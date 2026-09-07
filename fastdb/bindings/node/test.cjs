'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { Database, Record, Vector } = require('./index.cjs');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
test('native typed parameters and values preserve identities and binary64', () => {
  const db = new Database();
  db.execute('CREATE TABLE docs');
  const vectorBytes = Buffer.alloc(8); vectorBytes.writeFloatLE(1, 0); vectorBytes.writeFloatLE(2, 4);
  const doc = { id: new Record('docs', 9223372036854775807n), min: -9223372036854775808n,
    zero: -0, fraction: 1.25, delicate: 2.291712365432881e-9, yes: true, nil: null, text: 'docs:p1', bytes: Buffer.from([0,255]),
    vector: new Vector(vectorBytes), nested: { type: 'Integer', value: 'user text' }, list: [1n, 'a'] };
  db.execute('INSERT INTO docs DOCUMENT $doc', { $doc: doc });
  assert.deepEqual(db.exactlyOne('SELECT * FROM docs')[0], doc);
  const result = db.execute('SELECT $x AS x', { $x: 9223372036854775807n });
  assert.equal(result.rows[0][0], 9223372036854775807n);
  assert.equal(result.affected, 0n);
  assert.equal(result.transaction.after, 'autocommit');
  assert.throws(() => db.execute('SELECT $x', { $x: 9007199254740992 }), /bigint/);
  assert.throws(() => db.execute('SELECT $x', { $x: 1n << 63n }), /int64/);
  db.close();
});
test('validation, rollback and error transaction reports pass through frontend', () => {
  const db = new Database();
  db.execute('CREATE TABLE docs');
  db.execute('DEFINE FIELD value ON docs TYPE integer');
  db.execute('BEGIN');
  db.execute('INSERT INTO docs {id:docs:p1,value:1}');
  assert.throws(() => db.execute('INSERT INTO docs {id:docs:p1,value:2}'), error => error.code === 'FDB_CONSTRAINT' && error.transaction.after === 'active');
  assert.throws(() => db.execute("INSERT INTO docs {value:'bad'}"), error => error.code === 'FDB_VALIDATION' && error.transaction.after === 'active');
  db.execute('ROLLBACK');
  assert.deepEqual(db.all('SELECT * FROM docs'), []);
  db.execute('BEGIN');
  db.execute('INSERT INTO docs {value:2}');
  assert.throws(() => db.execute('SELECT array::append(1,2)'), error => error.transaction.after === 'autocommit');
  assert.deepEqual(db.all('SELECT * FROM docs'), []);
  db.close();
});
test('close is idempotent and persistence survives reopening', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'fastdb-node-'));
  try {
    const file = path.join(dir, 'test.db');
    let db = new Database(file);
    db.execute('CREATE TABLE docs');
    db.execute("INSERT INTO docs {id:docs:p1,name:'saved'}");
    db.close(); db.close();
    assert.throws(() => db.all('SELECT * FROM docs'), /closed/);
    db = new Database(file);
    assert.equal(db.first('SELECT name FROM docs')[0], 'saved');
    assert.equal(db.first('SELECT name FROM docs WHERE 0'), undefined);
    assert.throws(() => db.exactlyOne('SELECT name FROM docs WHERE 0'), /exactly one/);
    db.close();
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
test('prototype-looking object fields remain own data properties', () => {
  const db = new Database();
  const value = JSON.parse('{"__proto__":{"safe":true},"constructor":"data"}');
  assert.deepEqual(db.exactlyOne('SELECT $value AS value', { $value: value })[0], value);
  assert.equal({}.safe, undefined);
  db.close();
});
test('Node migration history retains int64 versions and failure atomicity', () => {
  const db = new Database();
  const plan = [{ version: 9007199254740993n, name: 'create', sql: 'CREATE TABLE docs;' }];
  assert.deepEqual(db.migrate(plan).applied, [9007199254740993n]);
  assert.equal(db.migrate(plan).alreadyApplied, 1);
  const pending = [...plan, { version: 9007199254740994n, name: 'fail', sql: 'INSERT INTO docs {value:1}; SELECT array::append(1,2);' }];
  assert.throws(() => db.migrate(pending), error => error.code === 'FDB_MIGRATION' && error.transaction.after === 'autocommit');
  assert.deepEqual(db.all('SELECT * FROM docs'), []);
  assert.throws(() => db.migrate([{ ...plan[0], sql: 'CREATE TABLE docs; -- edit' }]), error => error.code === 'FDB_VALIDATION');
  pending[1].sql = 'INSERT INTO docs {value:1};';
  assert.deepEqual(db.migrate(pending).applied, [9007199254740994n]);
  db.close();
});
test('Node transfers preserve values and roll back duplicate imports', () => {
  const source = new Database();
  source.execute('CREATE TABLE docs');
  source.execute('INSERT INTO docs DOCUMENT $doc', { $doc: { id: new Record('docs','p1'), value: 9223372036854775807n, zero: -0, bytes: Buffer.from([255]) } });
  for (const format of ['json', 'ndjson']) {
    const target = new Database();
    target.execute('CREATE TABLE docs');
    const payload = source.exportDocuments('docs', format);
    assert.equal(target.importDocuments('docs', payload, format).imported, 1);
    assert.deepEqual(target.all('SELECT * FROM docs'), source.all('SELECT * FROM docs'));
    assert.throws(() => target.importDocuments('docs', payload, format), error => error.transaction.after === 'autocommit');
    assert.equal(target.exportDocuments('docs', format), payload);
    target.execute('BEGIN');
    assert.throws(() => target.importDocuments('docs', 'bad input', format), error => error.transaction.after === 'active');
    target.execute('ROLLBACK');
    assert.throws(() => target.exportDocuments('docs', 'csv'), error => error.code === 'FDB_VALIDATION');
    target.close();
  }
  source.close();
});
test('async worker preserves submission order, typed values and graceful close', async t => {
  const { AsyncDatabase } = require('./index.cjs');
  const db = await AsyncDatabase.open();
  t.after(() => db.close());
  await db.migrate([{ version: 1n, name: 'create', sql: 'CREATE TABLE docs;' }]);
  const begin = db.execute('BEGIN');
  const insert = db.execute('INSERT INTO docs DOCUMENT $doc', { $doc: { id: new Record('docs','p1'), value: 9223372036854775807n, bytes: Buffer.from([255]) } });
  const read = db.exactlyOne('SELECT * FROM docs');
  const rollback = db.execute('ROLLBACK');
  const missing = db.all('SELECT * FROM docs');
  const [started, , row, , rows] = await Promise.all([begin,insert,read,rollback,missing]);
  assert.equal(started.transaction.after, 'active');
  assert.deepEqual(row[0].id, new Record('docs','p1'));
  assert.equal(row[0].value, 9223372036854775807n);
  assert.deepEqual(row[0].bytes, Buffer.from([255]));
  assert.deepEqual(rows, []);
  const queued = db.execute('INSERT INTO docs {id:docs:p2}');
  const close = db.close();
  assert.equal(db.close(), close);
  await assert.rejects(db.all('SELECT * FROM docs'), /closing or closed/);
  await queued; await close;
});
test('async worker leaves event loop responsive and rejects excess queued requests', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const db = await AsyncDatabase.open();
  try {
    await db.execute('CREATE TABLE numbers(x INTEGER)');
    await db.execute('INSERT INTO numbers VALUES ' + Array.from({length:100}, (_,i) => `(${i+1})`).join(','));
    let completed = false;
    const computation = db.execute('SELECT sum(a.x) FROM numbers a CROSS JOIN numbers b CROSS JOIN numbers c').finally(() => { completed = true; });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(completed, false);
    assert.deepEqual((await computation).rows, [[50500000n]]);
    const requests = Array.from({length: 257}, () => db.execute('SELECT 1'));
    const results = Promise.allSettled(requests);
    let ticked = false;
    await new Promise(resolve => setImmediate(() => { ticked = true; resolve(); }));
    const settled = await results;
    assert.equal(ticked, true);
    assert.equal(settled.filter(r => r.status === 'fulfilled').length, 256);
    assert.equal(settled[256].reason.code, 'FDB_LIMIT');
    assert.deepEqual(await db.exactlyOne('SELECT 2'), [2n]);
  } finally { await db.close(); }
});
test('async worker reports errors, transfers data and closes active transactions', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'fastdb-async-'));
  let db;
  try {
    await assert.rejects(AsyncDatabase.open(path.join(dir, 'missing', 'db')));
    const file = path.join(dir, 'test.db');
    db = await AsyncDatabase.open(file);
    await db.execute('CREATE TABLE docs');
    await db.execute('INSERT INTO docs {id:docs:p1,value:1}');
    const data = await db.exportDocuments('docs','ndjson');
    await db.execute('DELETE FROM docs:p1');
    assert.equal((await db.importDocuments('docs',data,'ndjson')).imported,1);
    await db.execute('BEGIN');
    await db.execute('INSERT INTO docs {id:docs:p2}');
    await assert.rejects(db.execute('SELECT array::append(1,2)'), e => e.transaction.after === 'autocommit');
    await db.execute('BEGIN');
    await db.execute('INSERT INTO docs {id:docs:p3}');
    await db.close();
    db = await AsyncDatabase.open(file);
    assert.equal((await db.all('SELECT * FROM docs')).length,1);
    await db.close();
  } finally { if (db) await db.close(); fs.rmSync(dir,{recursive:true,force:true}); }
});
test('independent async workers isolate connections and failures', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const databases = await Promise.all([AsyncDatabase.open(), AsyncDatabase.open()]);
  try {
    await Promise.all(databases.map(db => db.execute('CREATE TABLE docs')));
    await Promise.all(databases.map((db,i) => db.execute('INSERT INTO docs {value:$value}', { $value: BigInt(i) })));
    assert.deepEqual(await Promise.all(databases.map(db => db.exactlyOne('SELECT value FROM docs'))), [[0n], [1n]]);
    await assert.rejects(databases[0].execute('SELECT array::append(1,2)'));
    assert.deepEqual(await databases[1].exactlyOne('SELECT value FROM docs'), [1n]);
  } finally { await Promise.all(databases.map(db => db.close())); }
});
test('async interrupt reaches active native work without terminating its worker', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const db = await AsyncDatabase.open();
  let timer;
  try {
    assert.equal(db.interrupt(),true); // Idle requests must not poison later work.
    await db.execute('CREATE TABLE numbers(x INTEGER)');
    await db.execute('INSERT INTO numbers VALUES ' + Array.from({length:100},(_,i)=>`(${i})`).join(','));
    await db.execute('CREATE TABLE sink(x INTEGER)');
    const operation = db.execute('INSERT INTO sink SELECT a.x FROM numbers a CROSS JOIN numbers b CROSS JOIN numbers c');
    timer = setInterval(() => db.interrupt(), 2);
    await assert.rejects(operation, e => e.code === 'FDB_CANCELLED' && e.transaction.after === 'autocommit');
    clearInterval(timer); timer = undefined;
    assert.deepEqual(await db.exactlyOne('SELECT count(*) FROM sink'),[0n]);
    await db.execute('INSERT INTO sink VALUES (1)');
    assert.deepEqual(await db.exactlyOne('SELECT count(*) FROM sink'),[1n]);
  } finally { clearInterval(timer); await db.close(); }
  assert.equal(db.interrupt(),false);
});
test('worker transport failures settle pending requests and permit cleanup', () => {
  const { spawnSync } = require('node:child_process');
  const result = spawnSync(process.execPath, [require.resolve('./worker-faults.cjs')], { encoding: 'utf8', timeout: 10000 });
  assert.equal(result.status, 0, result.stderr || String(result.error));
  assert.match(result.stdout, /worker-faults-complete/);
});
test('nested record targets follow validation even without a reference index', () => {
  const db = new Database();
  try {
    db.execute('CREATE TABLE docs');
    for (const table of ['__fastdb_catalog', 'SQLITE_schema', 'bad\0target']) {
      assert.throws(() => db.execute('INSERT INTO docs DOCUMENT $doc', { $doc: { nested: [new Record(table, 'key')] } }), e => e.code === 'FDB_VALIDATION');
    }
    assert.deepEqual(db.all('SELECT * FROM docs'), []);
  } finally { db.close(); }
});
test('Node batches preserve offsets and stop at execution or encoding failures', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      const script = "-- é\nCREATE TABLE docs; BEGIN; INSERT INTO docs {id:docs:p1}; INSERT INTO docs {id:docs:p1}; COMMIT;";
      const reports = await db.executeBatch(script);
      assert.equal(reports.length,4);
      assert.equal(reports[0].offset,Buffer.byteLength('-- é\n'));
      assert.equal(reports[1].transaction.after,'active');
      assert(reports[3].error);
      assert.equal(reports[3].transaction.after,'active');
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT * FROM docs'),[]);
      const failedEncoding = await db.executeBatch('SELECT 1e308*1e308; INSERT INTO docs {id:docs:later};');
      assert.equal(failedEncoding.length,1);
      assert.equal(failedEncoding[0].error.code,'FDB_VALIDATION');
      assert.deepEqual(await db.all('SELECT * FROM docs'),[]);
      const prior=await db.executeBatch('INSERT INTO docs {id:docs:before}; SELECT * FROM docs;');
      assert.deepEqual(prior[1].result.rows[0][0].id,new Record('docs','before'));
      assert.equal(prior[0].result.affected,1n);
      if (db instanceof Database) assert.throws(() => db.executeBatch("INSERT INTO docs {id:docs:no}; SELECT 'unterminated"));
      else await assert.rejects(db.executeBatch("INSERT INTO docs {id:docs:no}; SELECT 'unterminated"));
      assert.equal((await db.all('SELECT * FROM docs')).length,1);
    } finally { await db.close(); }
  }
});

test('shared-file contention exposes busy codes and preserves committed values', () => {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'fastdb-contention-'));
  let a,b;
  try {
    const file=path.join(dir,'test.db');
    a=new Database(file); b=new Database(file);
    a.execute('CREATE TABLE docs');
    a.execute("INSERT INTO docs {id:docs:p1,value:1}");
    a.execute('BEGIN');
    a.execute("UPDATE docs:p1 {value:2}");
    assert.throws(() => b.execute("UPDATE docs:p1 {value:3}"), e => e.code==='FDB_BUSY' && e.transaction.after==='autocommit');
    assert.deepEqual(b.all('SELECT value FROM docs'),[[1n]]);
    a.execute('COMMIT');
    b.execute('BEGIN'); b.all('SELECT * FROM docs');
    a.execute("UPDATE docs:p1 {value:4}");
    assert.throws(() => b.execute("UPDATE docs:p1 {value:5}"), e => e.code==='FDB_BUSY_SNAPSHOT');
    b.execute('ROLLBACK');
    assert.deepEqual(b.all('SELECT value FROM docs'),[[4n]]);
  } finally { b?.close(); a?.close(); fs.rmSync(dir,{recursive:true,force:true}); }
});

test('sync and async SELECT profiles preserve typed values and bigint counters', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('BEGIN');
      for (let n = 0; n < 40; n++) {
        await db.execute('INSERT INTO docs (id,bucket,data,flag) VALUES ($id,$bucket,$data,$flag)', {
          $id: new Record('docs', BigInt(n)), $bucket: BigInt(n % 4),
          $data: new Uint8Array([0, 255]), $flag: true,
        });
      }
      await db.execute('COMMIT');
      const sql = 'SELECT id,data,flag FROM docs WHERE bucket=$bucket ORDER BY id';
      const parameters = { $bucket: 2n };
      const scan = await db.profileSelect(sql, parameters);
      assert.deepEqual(scan.result, await db.execute(sql, parameters));
      assert.equal(scan.result.rows.length, 10);
      for (const value of Object.values(scan.metrics)) assert.equal(typeof value, 'bigint');
      assert.equal(scan.metrics.rowsWritten, 0n);
      assert.ok(scan.metrics.fullscanSteps >= 39n);
      await db.execute('CREATE INDEX docs_bucket ON docs(bucket)');
      const indexed = await db.profileSelect(sql, parameters);
      assert.deepEqual(indexed.result, scan.result);
      assert.ok(indexed.metrics.rowsRead < scan.metrics.rowsRead);
      assert.ok(indexed.metrics.fullscanSteps < scan.metrics.fullscanSteps);
      assert.deepEqual((await db.profileSelect(sql, parameters)).metrics, indexed.metrics);
      const native = await db.profileSelect('SELECT $value AS value', { $value: new Uint8Array([2, 10]) });
      assert.deepEqual(native.result.rows, [[Buffer.from([2, 10])]]);
      await db.execute('BEGIN');
      await assert.rejects(Promise.resolve().then(() => db.profileSelect('DELETE FROM docs')), error => {
        assert.equal(error.code, 'FDB_UNSUPPORTED');
        assert.equal(error.transaction.after, 'active');
        return true;
      });
      assert.equal((await db.execute('SELECT count(*) FROM docs')).rows[0][0], 40n);
      await db.execute('ROLLBACK');
    } finally {
      await db.close();
    }
    await assert.rejects(Promise.resolve().then(() => db.profileSelect('SELECT 1')), /clos/i);
  }
});

test('sync and async collection audits preserve work and enforce bigint limits', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('CREATE INDEX docs_n ON docs(n)');
      assert.equal((await db.checkCollectionIntegrity('docs', {maxDocuments: 0n, maxEncodedBytes: 0n})).documents, 0n);
      await db.execute('INSERT INTO docs {id:docs:a,n:1}');
      const audit = await db.checkCollectionIntegrity('docs');
      assert.equal(audit.documents, 1n);
      assert.equal(audit.indexes, 1n);
      assert.equal(audit.indexEntries, 1n);
      assert.ok(audit.encodedBytes > 0n);
      assert.equal(audit.transaction.after, 'autocommit');
      assert.equal((await db.checkCollectionIntegrity('docs', {maxEncodedBytes: audit.encodedBytes})).encodedBytes, audit.encodedBytes);
      await db.execute('BEGIN');
      await db.execute('INSERT INTO docs {id:docs:b,n:null}');
      await assert.rejects(Promise.resolve().then(() => db.checkCollectionIntegrity('docs', {maxDocuments: 1n})), error => {
        assert.equal(error.code, 'FDB_LIMIT'); assert.equal(error.transaction.after, 'active'); return true;
      });
      assert.equal((await db.checkCollectionIntegrity('docs')).documents, 2n);
      for (const limits of [{maxDocuments: 1}, {maxDocuments: -1n}, {maxEncodedBytes: 1n << 64n}, {unknown: 1n}, null]) {
        await assert.rejects(Promise.resolve().then(() => db.checkCollectionIntegrity('docs', limits)), /integrity limit/);
      }
      await db.execute('ROLLBACK');
      assert.equal((await db.checkCollectionIntegrity('docs')).documents, 1n);
      await assert.rejects(Promise.resolve().then(() => db.checkCollectionIntegrity('missing')), error => error.code === 'FDB_NOT_FOUND');
    } finally { await db.close(); }
    await assert.rejects(Promise.resolve().then(() => db.checkCollectionIntegrity('docs')), /clos/i);
  }
});

test('recursive SQL reports parser depth errors through sync and worker clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('BEGIN');
      await db.execute('INSERT INTO docs {v:1}');
      for (const expression of ['NOT '.repeat(2000) + '1', 'CASE WHEN 1 THEN '.repeat(200) + '1' + ' ELSE 0 END'.repeat(200)]) {
        for (const suffix of ['', ' FROM docs']) {
          await assert.rejects(Promise.resolve().then(() => db.execute('SELECT ' + expression + suffix)), error => {
            if (suffix) assert.equal(error.code, 'FDB_UNSUPPORTED');
            else assert.match(error.message, /maximum depth 100/);
            assert.equal(error.transaction.after, 'active');
            return true;
          });
          await assert.rejects(Promise.resolve().then(() => db.profileSelect('SELECT ' + expression + suffix)), /maximum depth 100/);
        }
      }
      assert.equal((await db.exactlyOne('SELECT v FROM docs'))[0], 1n);
      await db.execute('ROLLBACK');
    } finally { await db.close(); }
  }
});
