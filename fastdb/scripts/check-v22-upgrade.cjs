'use strict';
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {createHash}=require('node:crypto');
const {execFileSync}=require('node:child_process');
const [oldPackage,newPackage,directory,phase,file]=process.argv.slice(2);
if(!oldPackage||!newPackage||!directory) throw Error('Usage: node check-v22-upgrade.cjs <published-2.0-or-2.1-package> <2.2-package> <new-output-directory>');
for(const value of [oldPackage,newPackage,directory]) assert.ok(path.isAbsolute(value));
const oldVersion=require(path.join(oldPackage,'package.json')).version;
assert.equal(require(path.join(newPackage,'package.json')).version,'2.2.0');
const hashes={
  '2.0.0':'11e472d275bb287bc4806e086d3ffed268bf170240b7f2ab6d981270e87f523d',
  '2.1.0':'e39e9f5ca3e22c1c4d5f594fd362bc523289c86a006d49f6cfed338e7f94ac5d',
};
assert.ok(hashes[oldVersion]);
const digest=file=>createHash('sha256').update(fs.readFileSync(file)).digest('hex');
for(const [name,expected] of Object.entries({
  'fastdb.node':hashes[oldVersion],
  'index.cjs':'d13a91b2c76ceb1fe0253c13ffd6249947bbfaf76addbb9a8fabb1799067fead',
  'worker.cjs':'1a4bd8044d7438ff296297832fcc592b281994cf12554598eaf505832431d655',
  'native.cjs':'2ab72dfe0ae4f1782d113fe210924f23f0fcba42094e5d6892ed999e9bc1d1a7',
})) assert.equal(digest(path.join(oldPackage,name)),expected,`${oldVersion} ${name}`);
const copy=(source,target)=>fs.copyFileSync(source,target,fs.constants.COPYFILE_EXCL);
if(!phase) {
  fs.mkdirSync(directory);
  const source=path.join(directory,'source.db');
  const previous=path.join(directory,'previous-backup.db');
  const candidate=path.join(directory,'candidate-backup.db');
  const phases=[];
  const run=(step,database)=>{
    execFileSync(process.execPath,[__filename,oldPackage,newPackage,directory,step,database],{stdio:'inherit',timeout:120000});
    phases.push(step);
  };
  run('seed',source); copy(source,previous); const previousHash=digest(previous);
  run('upgrade',source); run('candidate-reopen',source);
  copy(source,candidate); const candidateHash=digest(candidate);
  const newRestore=path.join(directory,'candidate-restore.db');copy(candidate,newRestore);
  run('candidate-restore',newRestore);run('restored-reopen',newRestore);run('candidate-reopen',source);
  const oldRestore=path.join(directory,'previous-restore.db');copy(previous,oldRestore);
  run('previous-restore',oldRestore);run('upgrade',oldRestore);run('candidate-reopen',oldRestore);
  const downgrade=path.join(directory,'downgrade-copy.db');copy(candidate,downgrade);run('reject-downgrade',downgrade);
  const catalog5=path.join(directory,'catalog5.db');run('catalog5-seed',catalog5);run('reject-catalog5',catalog5);
  assert.equal(digest(previous),previousHash);assert.equal(digest(candidate),candidateHash);
  const report={fromVersion:oldVersion,toVersion:'2.2.0',node:process.version,
    fromAddonSha256:hashes[oldVersion],toAddonSha256:digest(path.join(newPackage,'fastdb.node')),
    fromBackupSha256:previousHash,toBackupSha256:candidateHash,backupsUnchanged:true,
    explicitFtsReindex:true,reindexRollbackVerified:true,oldReaderRejectsCatalog5:true,phases,
    scope:'Offline upgrade from immutable published libraries, new catalog-5 writes, reopen and independent backup restore; no binary downgrade promise'};
  fs.writeFileSync(path.join(directory,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({ok:true,...report}));
} else {
  const old=['seed','previous-restore','reject-downgrade','reject-catalog5'].includes(phase);
  const {Database,Record,Vector}=require(old?oldPackage:newPackage);
  if(phase==='reject-downgrade'||phase==='reject-catalog5') {
    const expected=phase==='reject-catalog5'?/unsupported collection metadata version 5/:/unsupported collection metadata version [45]|unknown module name: 'fts'/;
    assert.throws(()=>{const db=new Database(file);try{db.all('SELECT * FROM next_docs');}finally{db.close();}},expected);
    console.log(`${phase}: passed`);process.exit(0);
  }
  const migration={version:1n,name:'previous-app',sql:[
    'CREATE TABLE users','DEFINE FIELD name ON users TYPE string REQUIRED','CREATE UNIQUE INDEX users_name ON users(name)',
    'CREATE TABLE docs','DEFINE FIELD title ON docs TYPE string REQUIRED CHECK(length(title)>0)',
    'DEFINE FIELD owner ON docs TYPE record<users>','DEFINE FIELD v ON docs TYPE vector<3>',
    'CREATE INDEX docs_owner ON docs(owner)','CREATE INDEX docs_city ON docs(profile.city)',
    'CREATE SEARCH INDEX docs_text ON docs(title) USING FULLTEXT',
    "CREATE SEARCH INDEX docs_vec ON docs(v) USING VECTOR WITH(dimensions=3,metric='cosine')",
    'CREATE SEARCH INDEX docs_geo ON docs(p) USING SPATIAL','DEFINE RELATION documents ON users FROM docs.owner',
    'CREATE TABLE events(id INTEGER PRIMARY KEY,message TEXT)','CREATE VIEW event_view AS SELECT id,message FROM events',
    "CREATE FUNCTION app::normalize(value string) RETURNS string LANGUAGE JAVASCRIPT AS 'return value.trim().toLowerCase();'",
  ].join('; ')+';'};
  const documents=[
    {id:new Record('docs','a'),title:'Fast river',owner:new Record('users','alice'),p:{type:'Point',coordinates:[100n,13n]},v:Vector.float32([1,0,0]),profile:{city:'Bangkok'},max:9223372036854775807n,min:-9223372036854775808n,zero:-0,tiny:Number.MIN_VALUE,binary:Buffer.from([0,255,128]),nil:null,flag:true,nested:{items:['literal',9n,false]}},
    {id:new Record('docs','b'),title:'Quiet ocean',owner:new Record('users','bob'),p:{type:'Point',coordinates:[-70n,40n]},v:Vector.float32([0,1,0]),profile:{city:'Boston'}},
  ];
  const db=new Database(file);
  function baseline() {
    assert.deepEqual(db.all('PRAGMA integrity_check'),[['ok']]);
    for(const document of documents) assert.deepEqual(db.collection('docs').get(document.id.key),document);
    assert.deepEqual(db.all('SELECT * FROM event_view WHERE id=1'),[[1n,'previous version']]);
    assert.equal(db.migrate([migration]).alreadyApplied,1);
    assert.throws(()=>db.migrate([{...migration,sql:migration.sql+' '}]),e=>e.code==='FDB_VALIDATION');
    assert.deepEqual(db.all("SELECT app::normalize(' ALICE ')"),[['alice']]);
    assert.deepEqual(db.all("SELECT d.id FROM docs d WHERE d.profile.city='Bangkok'"),[[new Record('docs','a')]]);
    assert.deepEqual(db.all("SELECT id FROM search::vector('docs_vec',vector32('[1,0,0]'),1)"),[[new Record('docs','a')]]);
    assert.deepEqual(db.all("SELECT id FROM search::near('docs_geo',geo::point(100,13),10)"),[[new Record('docs','a')]]);
    assert.deepEqual(db.exactlyOne("SELECT relation::fetch(users:alice,'documents',10)")[0],[documents[0]]);
    assert.throws(()=>db.execute("UPDATE docs:a {title:''}"));
    assert.throws(()=>db.execute("INSERT INTO users {id:users:duplicate,name:'Alice'}"));
  }
  const text=()=>assert.deepEqual(db.all("SELECT id FROM search::text('docs_text','river',10)"),[[new Record('docs','a')]]);
  function current() {
    baseline();text();
    for(const table of ['docs','users','next_docs']) db.checkCollectionIntegrity(table);
    assert.deepEqual(db.all('SELECT n,total,stamp,tags FROM next_docs'),[[2n,4n,7n,[2n,3n]]]);
    assert.deepEqual(db.all('SELECT id FROM next_docs WHERE array::contains(tags,3)'),[[new Record('next_docs','a')]]);
    assert.deepEqual(db.all("SELECT id FROM search::vector('next_vec',vector32('[1,0]'),1)"),[[new Record('next_docs','a')]]);
    assert.deepEqual(db.all("SELECT id FROM search::text('next_text','Alpha',1)"),[[new Record('next_docs','a')]]);
    assert.deepEqual(db.all('SELECT DISTINCT tags FROM next_docs'),[[[2n,3n]]]);
    assert.deepEqual(db.all('WITH RECURSIVE r(n) AS (SELECT n FROM next_docs UNION ALL SELECT n+1 FROM r WHERE n<4) SELECT n FROM r'),[[2n],[3n],[4n]]);
    assert.throws(()=>db.execute('UPDATE next_docs SET stamp=8'),e=>e.code==='FDB_VALIDATION');
  }
  try {
    db.execute('PRAGMA synchronous=FULL');
    if(phase==='catalog5-seed') {
      db.execute('CREATE TABLE next_docs');
      db.execute('DEFINE FIELD stamp ON next_docs TYPE integer DEFAULT 7 READONLY');
      db.execute('INSERT INTO next_docs {id:next_docs:a}');
      assert.deepEqual(db.all('SELECT stamp FROM next_docs'),[[7n]]);
    } else if(phase==='seed') {
      assert.deepEqual(db.migrate([migration]).applied,[1n]);
      db.execute("INSERT INTO users {id:users:alice,name:'Alice'}");db.execute("INSERT INTO users {id:users:bob,name:'Bob'}");
      for(const document of documents) db.execute('INSERT INTO docs DOCUMENT $doc',{$doc:document});
      db.execute("INSERT INTO events VALUES(1,'previous version')");baseline();text();
    } else if(phase==='upgrade') {
      baseline();
      assert.throws(text,e=>e.code==='FDB_STORAGE'&&e.message.includes('REINDEX'));
      db.execute('BEGIN');db.execute("INSERT INTO events VALUES(2,'prior work')");
      for(const sql of ["INSERT INTO docs {title:'blocked'}","UPDATE docs SET title='blocked'",'DELETE FROM docs']) {
        assert.throws(()=>db.execute(sql),e=>e.code==='FDB_STORAGE'&&e.message.includes('REINDEX'));
        assert.deepEqual(db.all('SELECT message FROM events WHERE id=2'),[['prior work']]);
      }
      db.execute('REINDEX docs_text');text();db.execute('ROLLBACK');
      assert.throws(text,e=>e.code==='FDB_STORAGE'&&e.message.includes('REINDEX'));
      assert.deepEqual(db.all('SELECT message FROM events WHERE id=2'),[]);
      db.execute('REINDEX docs_text');text();
      db.execute('BEGIN');
      db.execute('CREATE TABLE next_docs');
      db.execute('DEFINE FIELD stamp ON next_docs TYPE integer DEFAULT 7 READONLY');
      db.execute('DEFINE FIELD total ON next_docs TYPE integer VALUE (n*2)');
      db.execute("INSERT INTO next_docs {id:next_docs:a,n:1,uid:'one',tags:[1],body:'Alpha beta',v:vector32('[1,0]')}");
      db.execute('CREATE UNIQUE INDEX next_pair ON next_docs(n,uid)');
      db.execute('CREATE SEARCH INDEX next_tags ON next_docs(tags) USING ARRAY');
      db.execute("CREATE SEARCH INDEX next_vec ON next_docs(v) USING VECTOR WITH(dimensions=2,metric='l2',quantization='f16')");
      db.execute("CREATE SEARCH INDEX next_text ON next_docs(body) USING FULLTEXT WITH(tokenizer='simple')");
      db.execute("INSERT INTO next_docs(n,uid,tags) VALUES(1,'one',array::new(2)) ON CONFLICT(n,uid) DO UPDATE SET n=2,tags=excluded.tags");
      db.execute("UPDATE next_docs:a PATCH [{op:'add',path:'/tags/-',value:3}]");
      db.execute('COMMIT');current();
    } else if(phase==='previous-restore') {baseline();text();}
    else if(phase==='candidate-restore') {
      current();db.execute('BEGIN');db.execute('DELETE FROM next_docs');db.execute('ROLLBACK');current();
      db.execute("INSERT INTO events VALUES(3,'restored')");
    } else if(['candidate-reopen','restored-reopen'].includes(phase)) {
      current();assert.deepEqual(db.all('SELECT message FROM events WHERE id=3'),phase==='restored-reopen'?[['restored']]:[]);
    } else throw Error(`Unknown phase ${phase}`);
    assert.deepEqual(db.all('PRAGMA wal_checkpoint(TRUNCATE)'),[[0n,0n,0n]]);
    assert.equal(fs.statSync(file+'-wal').size,0);
    console.log(`${phase}: passed`);
  } finally {db.close();}
}
