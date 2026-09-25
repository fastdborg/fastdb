'use strict';
// Separate processes keep the V1 and V2 native libraries out of the same VM.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { execFileSync } = require('node:child_process');
const [oldPackage, newPackage, directory, phase, file] = process.argv.slice(2);
if (!oldPackage || !newPackage || !directory) {
  throw new Error('Usage: node check-v2-upgrade.cjs <absolute-v1-package> <absolute-v2-package> <new-output-directory>');
}
for (const value of [oldPackage, newPackage, directory]) assert.ok(path.isAbsolute(value), 'Use absolute paths');
const digest = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const copy = (source, destination) => fs.copyFileSync(source, destination, fs.constants.COPYFILE_EXCL);
const fixturePath = path.join(directory, 'fixture.json');
const migration = {
  version: 1n, name: 'v1-application', sql: [
    'CREATE TABLE users',
    'DEFINE FIELD name ON users TYPE string REQUIRED',
    'DEFINE FIELD score ON users TYPE integer CHECK(score>0)',
    'CREATE UNIQUE INDEX users_name ON users(name)',
    'CREATE TABLE docs',
    'DEFINE FIELD title ON docs TYPE string REQUIRED CHECK(length(title)>0)',
    'DEFINE FIELD owner ON docs TYPE record<users>',
    'DEFINE FIELD v ON docs TYPE vector<3>',
    'CREATE INDEX docs_owner ON docs(owner)',
    'CREATE INDEX docs_city ON docs(profile.city)',
    'CREATE TABLE events(id INTEGER PRIMARY KEY, message TEXT)',
    'CREATE VIEW event_view AS SELECT id,message FROM events',
  ].join('; ') + ';',
};

if (!phase) {
  // Refuse to overwrite a fixture or backup from a previous run.
  fs.mkdirSync(directory);
  const source = path.join(directory, 'source.db');
  const backup = path.join(directory, 'v1-backup.db');
  function run(step, database) {
    execFileSync(process.execPath, [__filename, oldPackage, newPackage, directory, step, database], { stdio: 'inherit' });
  }
  run('seed', source);
  copy(source, backup);
  const originalHash = digest(backup);
  run('upgrade', source);
  run('v2-check', source);
  const v2Backup = path.join(directory, 'v2-backup.db');
  copy(source, v2Backup);
  const v2Hash = digest(v2Backup);
  const downgrade = path.join(directory, 'downgrade-copy.db');
  copy(v2Backup, downgrade);
  run('reject-fts-downgrade', downgrade);
  run('strip-fts-for-downgrade', downgrade);
  run('reject-downgrade', downgrade);
  const restored = path.join(directory, 'restored-v2.db');
  copy(v2Backup, restored);
  run('v2-restore', restored);
  run('v2-restored-check', restored);
  run('v2-check', source);
  const oldRestored = path.join(directory, 'restored-v1.db');
  copy(backup, oldRestored);
  run('v1-restore', oldRestored);
  run('v1-restored-check', oldRestored);
  assert.equal(digest(backup), originalHash, 'V1 backup must remain unchanged');
  assert.equal(digest(v2Backup), v2Hash, 'V2 backup must remain unchanged');
  const report = {
    v1AddonSha256: digest(path.join(oldPackage, 'fastdb.node')),
    v2AddonSha256: digest(path.join(newPackage, 'fastdb.node')),
    v1BackupSha256: originalHash, v2BackupSha256: v2Hash,
    node: process.version, fixture: fixturePath,
    phases: ['seed', 'upgrade', 'v2-check', 'reject-fts-downgrade', 'strip-fts-for-downgrade', 'reject-downgrade', 'v2-restore', 'v2-restored-check', 'v1-restore', 'v1-restored-check'],
  };
  fs.writeFileSync(path.join(directory, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ ok: true, directory, ...report }, null, 2));
} else {
  const old = phase === 'seed' || phase.startsWith('reject-') || phase.startsWith('v1-');
  const { Database, Record, Vector } = require(old ? oldPackage : newPackage);
  const normalize = value => {
    if (typeof value === 'bigint') return { integer: String(value) };
    if (typeof value === 'number') { const bytes = Buffer.alloc(8); bytes.writeDoubleLE(value); return { binary64: bytes.toString('hex') }; }
    if (value instanceof Record) return { record: [value.table, normalize(value.key)] };
    if (value instanceof Vector) return { vector: Buffer.from(value.bytes).toString('hex') };
    if (value instanceof Uint8Array) return { binary: Buffer.from(value).toString('hex') };
    if (Array.isArray(value)) return value.map(normalize);
    if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([key, item]) => [key, normalize(item)]));
    return value;
  };
  const documents = [
    { id: new Record('docs', 'a'), title: 'Fast river', owner: new Record('users', 'alice'),
      p: { type: 'Point', coordinates: [100n, 13n] }, v: Vector.float32([1, 0, 0]), profile: { city: 'Bangkok' },
      max: 9223372036854775807n, min: -9223372036854775808n, zero: -0, tiny: Number.MIN_VALUE,
      fraction: 1.0000000000000002, binary: Buffer.from([0, 255, 128]), nil: null, flag: true,
      nested: { type: 'Record', value: ['literal', 9n, null, false] },
      vectors: [Vector.float64([0.1, 0.2, 0.3]), Vector.sparse32([0, 2, 0]), Vector.quantized8([1, 2, 3]), Vector.bit1([1, -1, 1])],
    },
    { id: new Record('docs', 1n), title: 'Quiet ocean', owner: new Record('users', 'bob'),
      p: { type: 'Point', coordinates: [-70n, 40n] }, v: Vector.float32([0, 1, 0]), profile: { city: 'Boston' } },
  ];
  if (phase === 'reject-downgrade' || phase === 'reject-fts-downgrade') {
    assert.throws(() => {
      const db = new Database(file);
      try { db.execute('SELECT * FROM docs'); } finally { db.close(); }
    }, phase === 'reject-fts-downgrade' ? /unknown module name: 'fts'/ : /unsupported collection metadata version 3/);
    console.log(`${phase}: V1 rejects the upgraded database`);
  } else {
    const db = new Database(file);
    let fixture = phase === 'seed' ? null : JSON.parse(fs.readFileSync(fixturePath, 'utf8'));
    function checkpoint() {
      assert.deepEqual(db.all('PRAGMA wal_checkpoint(TRUNCATE)'), [[0n, 0n, 0n]]);
      assert.equal(fs.statSync(file + '-wal').size, 0);
    }
    function assertBaseline(upgraded = false) {
      assert.deepEqual(db.all('PRAGMA integrity_check'), [['ok']]);
      for (const document of documents) assert.deepEqual(db.collection('docs').get(document.id.key), document);
      assert.equal(Object.hasOwn(db.collection('docs').get(1n), 'nil'), false);
      for (const table of ['users', 'docs']) {
        assert.equal(db.checkCollectionIntegrity(table).documents, 2n);
        assert.equal(db.exportDocuments(table), fixture.exports[table]);
        if (!upgraded) {
          const info = db.all(`INFO FOR TABLE ${table}`);
          // V2 adds these two inspection fields without changing V1 definitions.
          if (!old) {
            assert.deepEqual(info[0][0].relations, []);
            delete info[0][0].relations;
            for (const index of info[0][0].indexes) { assert.equal(index.kind, 'scalar'); delete index.kind; }
          }
          assert.deepEqual(normalize(info), fixture.info[table]);
        }
      }
      assert.deepEqual(db.all("SELECT d.id FROM docs d WHERE d.profile.city='Bangkok'"), [[new Record('docs', 'a')]]);
      assert.deepEqual(db.all('SELECT * FROM event_view WHERE id=1'), [[1n, 'created in V1']]);
      if (db.execute('SELECT 1').transaction.before === 'autocommit') {
        assert.equal(db.migrate([migration]).alreadyApplied, 1);
        assert.throws(() => db.migrate([{ ...migration, sql: migration.sql + ' ' }]), error => error.code === 'FDB_VALIDATION');
      } else assert.throws(() => db.migrate([migration]), /requires autocommit/);
      for (const sql of ["UPDATE users:alice {score:0}", "INSERT INTO users {id:users:duplicate,name:'Alice'}", "INSERT INTO users {id:users:invalid,score:1}"]) assert.throws(() => db.execute(sql));
      assert.deepEqual(db.all('SELECT count(*) FROM users'), [[2n]]);
    }
    function assertV2() {
      assertBaseline(true);
      assert.deepEqual(db.all("SELECT id FROM search::near('docs_geo',geo::point(100,13),10)"), [[new Record('docs', 'a')]]);
      assert.deepEqual(db.all("SELECT id FROM search::text('docs_text','river',10)"), [[new Record('docs', 'a')]]);
      assert.deepEqual(db.all("SELECT id FROM search::vector('docs_vec',vector32('[1,0,0]'),1)"), [[new Record('docs', 'a')]]);
      assert.deepEqual(db.exactlyOne("SELECT relation::fetch(users:alice,'documents',10)")[0], [documents[0]]);
      assert.deepEqual(db.exactlyOne('SELECT docs:a { title, owner.* AS owner }'), ['Fast river', { id: new Record('users', 'alice'), name: 'Alice', score: 7n }]);
      assert.equal(db.exactlyOne('SELECT geo::cell(p,7) FROM docs WHERE id=docs:a')[0], db.exactlyOne('SELECT geo::cell(geo::point(100,13),7)')[0]);
    }
    try {
      if (phase === 'seed') {
        assert.deepEqual(db.migrate([migration]).applied, [1n]);
        db.execute("INSERT INTO users {id:users:alice,name:'Alice',score:7}");
        db.execute("INSERT INTO users {id:users:bob,name:'Bob',score:2}");
        for (const document of documents) db.execute('INSERT INTO docs DOCUMENT $doc', { $doc: document });
        for (const document of documents) assert.deepEqual(db.collection('docs').get(document.id.key), document);
        db.execute("INSERT INTO events VALUES(1,'created in V1')");
        const snapshot = { migration: { ...migration, version: 1 }, exports: {}, info: {} };
        for (const table of ['users', 'docs']) {
          snapshot.exports[table] = db.exportDocuments(table);
          snapshot.info[table] = normalize(db.all(`INFO FOR TABLE ${table}`));
          assert.equal(db.checkCollectionIntegrity(table).documents, 2n);
        }
        fs.writeFileSync(fixturePath, JSON.stringify(snapshot, null, 2) + '\n', { flag: 'wx' });
        fixture = snapshot;
        assertBaseline();
      } else if (phase === 'upgrade') {
        assertBaseline();
        const schema = [
          'CREATE SEARCH INDEX docs_geo ON docs(p) USING SPATIAL',
          "CREATE SEARCH INDEX docs_vec ON docs(v) USING VECTOR WITH (dimensions=3,metric='cosine')",
          'CREATE SEARCH INDEX docs_text ON docs(title) USING FULLTEXT',
          'DEFINE RELATION documents ON users FROM docs.owner',
        ];
        db.execute('BEGIN');
        schema.forEach(sql => db.execute(sql));
        assertV2();
        db.execute('ROLLBACK');
        assertBaseline();
        assert.throws(() => db.execute("SELECT id FROM search::near('docs_geo',geo::point(100,13),10)"));
        schema.forEach(sql => db.execute(sql));
        assertV2();
        db.execute('BEGIN');
        db.execute("UPDATE docs:a {title:'Temporary meadow',p:geo::point(0,0),v:vector32('[0,0,1]'),owner:users:bob}");
        assert.deepEqual(db.all("SELECT id FROM search::text('docs_text','river',10)"), []);
        assert.deepEqual(db.all("SELECT id FROM search::near('docs_geo',geo::point(100,13),10)"), []);
        assert.deepEqual(db.exactlyOne("SELECT relation::fetch(users:alice,'documents',10)")[0], []);
        db.execute('DELETE FROM docs:1');
        db.execute("INSERT INTO events VALUES(2,'rolled back')");
        db.execute('ROLLBACK');
        assertV2();
        db.execute("INSERT INTO events VALUES(2,'upgraded in V2')");
      } else if (phase === 'strip-fts-for-downgrade') {
        db.execute('DROP INDEX docs_text');
        assertBaseline(true);
      } else if (phase.startsWith('v2-')) {
        assertV2();
        assert.deepEqual(db.all('SELECT * FROM event_view WHERE id=2'), [[2n, 'upgraded in V2']]);
        if (phase === 'v2-restore') {
          db.execute('BEGIN');
          db.execute('DELETE FROM docs:a');
          db.execute('ROLLBACK');
          assertV2();
          db.execute("INSERT INTO events VALUES(3,'written after V2 restore')");
        } else if (phase === 'v2-restored-check') {
          assert.deepEqual(db.all('SELECT message FROM events WHERE id=3'), [['written after V2 restore']]);
        } else assert.deepEqual(db.all('SELECT count(*) FROM events'), [[2n]]);
      } else if (phase.startsWith('v1-')) {
        assertBaseline();
        if (phase === 'v1-restore') {
          assert.deepEqual(db.all('SELECT count(*) FROM events'), [[1n]]);
          db.execute('BEGIN');
          db.execute('DELETE FROM docs:a');
          db.execute('ROLLBACK');
          assertBaseline();
          db.execute("INSERT INTO events VALUES(2,'written after V1 restore')");
        } else assert.deepEqual(db.all('SELECT message FROM events WHERE id=2'), [['written after V1 restore']]);
      } else throw new Error(`Unknown phase: ${phase}`);
      checkpoint();
      console.log(`${phase}: passed`);
    } finally { db.close(); }
  }
}
