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
  await assert.rejects(db.all('SELECT * FROM docs'), error => error.code === 'FDB_CLOSED' && !Object.hasOwn(error,'transaction'));
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
            assert.equal(error.code, 'FDB_ENGINE');
            assert.match(error.message, /maximum depth 100/);
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

test('typed VALUES CTEs preserve mixed values and atomic inserts in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      const id = new Record('docs', 7n);
      const data = Buffer.from([0,255]);
      const input = {$id:id, $data:data, $flag:true, $array:[1n,null]};
      const result = await db.execute('WITH v(n,x) AS (VALUES (1,$id),(2,$data),(3,$flag),(4,$array),(5,NULL)) SELECT x FROM v ORDER BY n', input);
      assert.deepEqual(result.rows, [[id],[data],[true],[[1n,null]],[null]]);
      const union = await db.execute('SELECT $id AS x UNION ALL SELECT $data UNION ALL SELECT $flag', {$id:id, $data:data, $flag:true});
      assert.deepEqual(union.rows, [[id],[data],[true]]);
      assert.deepEqual((await db.execute('SELECT $id AS x UNION SELECT $id', {$id:id})).rows, [[id]]);
      assert.deepEqual((await db.execute('SELECT $id AS x INTERSECT SELECT $data', {$id:id, $data:data})).rows, []);
      assert.deepEqual((await db.execute('SELECT $id AS x EXCEPT SELECT $data', {$id:id, $data:data})).rows, [[id]]);
      await db.execute('CREATE TABLE docs');
      await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
      await db.execute('BEGIN');
      await db.execute('WITH v(id,n) AS (VALUES ($id,1)) INSERT INTO docs(id,n) SELECT id,n FROM v', {$id:id});
      await assert.rejects(Promise.resolve().then(() => db.execute('WITH v(id,n) AS (VALUES (docs:b,2),(docs:c,1)) INSERT INTO docs(id,n) SELECT id,n FROM v')), error => error.code === 'FDB_CONSTRAINT' && error.transaction.after === 'active');
      assert.equal((await db.checkCollectionIntegrity('docs')).documents, 1n);
      assert.equal((await db.exactlyOne('SELECT n FROM docs'))[0], 1n);
      await db.execute('ROLLBACK');
      assert.equal((await db.checkCollectionIntegrity('docs')).documents, 0n);
    } finally { await db.close(); }
  }
});

test('vector factories match native encodings and work in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const factories = [['float32','vector32'],['float64','vector64'],['sparse32','vector32_sparse'],['quantized8','vector8'],['bit1','vector1bit']];
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE points');
      for (const [factory, sql] of factories) {
        for (const values of [[1,0,-1], new Float32Array([0.1,0.2,0.3]), new Float64Array([2.5,2.5,2.5]), new Array(9).fill(0)]) {
          const vector = Vector[factory](values);
          assert(vector instanceof Vector);
          const text = JSON.stringify(Array.from(values));
          const native = await db.exactlyOne(`SELECT ${sql}($text)`, {$text:text});
          assert.deepEqual(vector.bytes, native[0]);
          await db.execute('INSERT INTO points(v) VALUES ($v)', {$v:vector});
          assert.deepEqual((await db.exactlyOne('SELECT $v AS v', {$v:vector}))[0], vector);
        }
      }
      assert.equal((await db.checkCollectionIntegrity('points')).documents, 20n);
    } finally { await db.close(); }
  }
  for (const [factory] of factories) {
    for (const input of [[], new Float64Array(65537)]) assert.throws(() => Vector[factory](input), RangeError);
    for (const input of [[NaN],[Infinity],[-Infinity],[1n],['1'],[undefined],new Array(2), new Uint8Array([1])]) assert.throws(() => Vector[factory](input), TypeError);
    assert(Vector[factory](new Float32Array(65536)) instanceof Vector);
  }
  assert.throws(() => Vector.float32([Number.MAX_VALUE]), RangeError);
  assert(Vector.float64([Number.MAX_VALUE]) instanceof Vector);
  assert(Object.is(Vector.float32([-0]).bytes.readFloatLE(), -0));
  assert(Object.is(Vector.float64([-0]).bytes.readDoubleLE(), -0));
  const input = [1,2,3]; const vector = Vector.float32(input); input[0] = 99;
  assert.equal(vector.bytes.readFloatLE(0), 1);
  const { vectorFromComponents } = require('./fastdb.node');
  for (const bytes of [Buffer.alloc(0),Buffer.alloc(7),Buffer.alloc(65537*8)]) assert.throws(() => vectorFromComponents('float32', bytes));
  assert.throws(() => vectorFromComponents('unknown', Buffer.alloc(8)));
  const invalid = Buffer.alloc(8); invalid.writeDoubleLE(NaN);
  for (const [factory] of factories) assert.throws(() => vectorFromComponents(factory, invalid));
});

test('vector factories preserve precision boundaries and reject conversion overflow locally', () => {
  const values = [1 + 2 ** -24, 1 + 3 * 2 ** -24, 2 ** -150, 3 * 2 ** -150, -(2 ** -150), -0];
  const expectedBits = [0x3f800000, 0x3f800002, 0, 2, 0x80000000, 0x80000000];
  const dense = Vector.float32(values);
  assert.deepEqual(values.map((_, i) => dense.bytes.readUInt32LE(i * 4)), expectedBits);
  const precise = Vector.float64([Number.MIN_VALUE, -Number.MIN_VALUE, 1 + Number.EPSILON, Number.MAX_VALUE]);
  assert.deepEqual([0,1,2,3].map(i => precise.bytes.readBigUInt64LE(i * 8)), [1n,0x8000000000000001n,0x3ff0000000000001n,0x7fefffffffffffffn]);
  assert.equal(Vector.bit1(values).bytes[0], 0b1011);
  const db = new Database();
  try {
    for (const [factory, sql] of [['sparse32','vector32_sparse'],['quantized8','vector8'],['bit1','vector1bit']]) {
      const native = db.exactlyOne(`SELECT ${sql}($bytes)`, {$bytes:dense.bytes})[0];
      assert.deepEqual(Vector[factory](values).bytes, native);
    }
    db.execute('CREATE TABLE docs');
    db.execute('BEGIN');
    db.execute('INSERT INTO docs(n) VALUES (7)');
    const max32 = (2 - 2 ** -23) * 2 ** 127;
    assert.throws(() => Vector.quantized8([-max32,max32]));
    assert(Vector.quantized8([max32,max32]) instanceof Vector);
    const result = db.execute('SELECT n FROM docs');
    assert.equal(result.transaction.after, 'active');
    assert.deepEqual(result.rows, [[7n]]);
    db.execute('ROLLBACK');
    assert.equal(db.checkCollectionIntegrity('docs').documents, 0n);
  } finally { db.close(); }
});

test('sparse entry vectors validate indices and roundtrip through both clients', async () => {
  const entries = [[0, 1 + 2 ** -24], [1, -0], [3, -2]];
  const vector = Vector.sparse32Entries(5, entries);
  assert.deepEqual(vector.bytes, Vector.sparse32([1, 0, 0, -2, 0]).bytes);
  entries[0][1] = 99;
  assert.equal(vector.bytes.readFloatLE(), 1);
  assert.equal(Vector.sparse32Entries(65536, []).bytes.length, 5);
  assert.equal(Vector.sparse32Entries(65536, [[0, -0], [65535, 0]]).bytes.length, 5);
  const last = Vector.sparse32Entries(65536, [[65535, 1]]);
  assert.equal(last.bytes.length, 13);
  assert.equal(last.bytes.readUInt32LE(4), 65535);
  assert.equal(last.bytes.readUInt32LE(8), 65536);
  for (const dimensions of [0, -1, 1.5, 65537, NaN, Infinity, '3', 3n]) assert.throws(() => Vector.sparse32Entries(dimensions, []), RangeError);
  for (const input of [null, {}, new Float32Array(2), [null], [[0]], [[0, 1, 2]], new Array(1), [[0, NaN]], [[0, Infinity]], [[0, 1n]]]) assert.throws(() => Vector.sparse32Entries(3, input), TypeError);
  for (const input of [[[0, 0], [0, -0]], [[1, 1], [0, 2]], [[-1, 1]], [[3, 1]], [[0.5, 1]], [[NaN, 1]], [['0', 1]], [[0, Number.MAX_VALUE]], [[0, 1], [1, 1], [2, 1], [3, 1]]]) assert.throws(() => Vector.sparse32Entries(3, input), RangeError);
  const { vectorFromSparseEntries } = require('./fastdb.node');
  for (const dims of [0, 1.5, 65537, NaN, Infinity]) assert.throws(() => vectorFromSparseEntries(dims, Buffer.alloc(0)));
  for (const bytes of [Buffer.alloc(1), Buffer.alloc(13), Buffer.alloc(4 * 12)]) assert.throws(() => vectorFromSparseEntries(3, bytes));
  const invalid = Buffer.alloc(12);
  invalid.writeUInt32LE(3);
  assert.throws(() => vectorFromSparseEntries(3, invalid));
  invalid.writeUInt32LE(0);
  for (const value of [NaN, Infinity, Number.MAX_VALUE]) {
    invalid.writeDoubleLE(value, 4);
    assert.throws(() => vectorFromSparseEntries(3, invalid));
  }
  assert.throws(() => vectorFromSparseEntries(3, Buffer.alloc(24))); // Duplicate zero-valued indices.
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE points');
      await db.execute('DEFINE FIELD v ON points TYPE vector<5> REQUIRED');
      await db.execute('INSERT INTO points(v) VALUES ($v)', {$v:vector});
      assert.deepEqual((await db.exactlyOne('SELECT v FROM points'))[0], vector);
      assert.equal((await db.exactlyOne('SELECT vector_extract(vector32(v)) AS v FROM points'))[0], '[1,0,0,-2,0]');
      assert.equal((await db.checkCollectionIntegrity('points')).documents, 1n);
      await db.execute('BEGIN');
      assert.throws(() => Vector.sparse32Entries(5, [[5, 1]]), RangeError);
      assert.deepEqual((await db.exactlyOne('SELECT v FROM points'))[0], vector);
      await db.execute('ROLLBACK');
    } finally { await db.close(); }
  }
});


test('typed scalar subqueries preserve values in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('INSERT INTO docs {id:docs:a,n:1,link:docs:b,items:[1,true]}');
      const row = await db.exactlyOne('SELECT (SELECT link FROM docs) AS link,(SELECT items FROM docs) AS items,(SELECT n FROM docs WHERE n=99) AS missing');
      assert.deepEqual(row, [new Record('docs','b'), [1n,true], null]);
      assert.deepEqual(await db.exactlyOne('SELECT n FROM docs WHERE n=(SELECT max(n) FROM docs)'), [1n]);
      assert.deepEqual(await db.exactlyOne('SELECT DISTINCT n FROM docs LIMIT (SELECT $limit FROM docs) OFFSET (SELECT $offset FROM docs)', {$limit:1n,$offset:0n}), [1n]);
      assert.deepEqual(await db.exactlyOne('SELECT n FROM docs UNION SELECT n FROM docs ORDER BY n LIMIT (SELECT $limit FROM docs)', {$limit:1n}), [1n]);
      assert.deepEqual(await db.exactlyOne('SELECT EXISTS (SELECT link,items FROM docs) AS present,NOT EXISTS (SELECT n FROM docs WHERE n=99) AS absent'), [1n,1n]);
      assert.deepEqual(await db.exactlyOne('SELECT docs:b IN (SELECT link FROM docs) AS present,2 NOT IN (SELECT n FROM docs) AS absent,NULL IN (SELECT n FROM docs) AS unknown'), [1n,1n,null]);
      for (const value of [new Record('docs','key'), [1n,true], {ok:true}, Buffer.from([0,255]), Vector.float32([1,0])]) {
        assert.deepEqual(await db.exactlyOne('SELECT (SELECT $v AS v) AS v', {$v:value}), [value]);
        assert.deepEqual(await db.exactlyOne('WITH chosen AS (SELECT $v AS v) SELECT v FROM chosen', {$v:value}), [value]);
      }
      assert.deepEqual(await db.exactlyOne('SELECT ? AS n,(SELECT ? AS v) AS v', {'?1':3n,'?2':[true]}), [3n,[true]]);
      await db.execute('CREATE TABLE affinity_docs');
      await db.execute('INSERT INTO affinity_docs(n) VALUES (2)');
      assert.deepEqual(await db.exactlyOne("SELECT CAST('2' AS TEXT) IN (SELECT n FROM affinity_docs) AS v"), [0n]);
      await db.execute('CREATE TABLE affinity_native(n TEXT)');
      await db.execute("INSERT INTO affinity_native VALUES ('2')");
      assert.deepEqual(await db.exactlyOne('SELECT (SELECT n FROM affinity_native) AS v FROM docs'), ['2']);
      assert.deepEqual(await db.exactlyOne('SELECT EXISTS (SELECT n FROM affinity_native WHERE n=$n) AS present FROM docs', {$n:'2'}), [1n]);
      assert.deepEqual(await db.exactlyOne('SELECT (SELECT $v AS v FROM affinity_native) AS v', {$v:new Record('docs','native-source')}), [new Record('docs','native-source')]);
      assert.deepEqual(await db.exactlyOne('SELECT (SELECT $v AS v FROM affinity_native) AS v', {$v:[1n,true]}), [[1n,true]]);
      assert.deepEqual(await db.exactlyOne('WITH chosen AS (SELECT $v AS v FROM affinity_native) SELECT v FROM chosen', {$v:new Record('docs','cte-native')}), [new Record('docs','cte-native')]);
      assert.deepEqual(await db.exactlyOne('SELECT n IN (SELECT +n FROM affinity_docs) AS v FROM affinity_native'), [1n]);
      await db.execute('CREATE TABLE scalar_numeric(n INTEGER)');
      await db.execute('INSERT INTO scalar_numeric VALUES (2)');
      assert.deepEqual(await db.exactlyOne("SELECT '2'=(SELECT n FROM scalar_numeric) AS v FROM docs"), [1n]);
      assert.deepEqual(await db.exactlyOne("SELECT '2'=((SELECT n FROM scalar_numeric) COLLATE NOCASE) AS v FROM docs"), [1n]);
    } finally { await db.close(); }
  }
});

test('AbortSignal cancels only its queued or active query and cleans up listeners', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const { getEventListeners } = require('node:events');
  const db = await AsyncDatabase.open();
  let timer;
  try {
    await db.execute('CREATE TABLE numbers(n INTEGER)');
    await db.execute('INSERT INTO numbers VALUES ' + Array.from({length:100},(_,i)=>`(${i})`).join(','));
    await db.execute('CREATE TABLE docs');
    await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
    await db.execute('BEGIN');
    await db.execute('INSERT INTO docs {id:docs:prior,n:9}');
    const active = new AbortController(), queued = new AbortController();
    // A user listener must not suppress the driver's cancellation listener.
    active.signal.addEventListener('abort', event => event.stopImmediatePropagation(), {once:true});
    const first = db.execute('INSERT INTO docs(n) SELECT a.n FROM numbers a CROSS JOIN numbers b CROSS JOIN numbers c', {}, {signal:active.signal});
    const second = db.execute('INSERT INTO docs {n:77}', {}, {signal:queued.signal});
    const third = db.execute('INSERT INTO docs {n:88}');
    const outcomes = Promise.allSettled([first, second, third]);
    queued.abort();
    timer = setTimeout(() => active.abort(), 10);
    const [a,b,c] = await outcomes;
    for (const result of [a,b]) {
      assert.equal(result.status, 'rejected');
      assert.equal(result.reason.code, 'FDB_CANCELLED');
      assert.equal(result.reason.transaction.after, 'active');
    }
    assert.equal(c.status,'fulfilled');
    assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'), [[9n],[88n]]);
    assert.equal((await db.checkCollectionIntegrity('docs')).documents,2n);
    assert.equal(getEventListeners(active.signal,'abort').length,0);
    assert.equal(getEventListeners(queued.signal,'abort').length,0);
    const finished = new AbortController();
    assert.deepEqual(await db.exactlyOne('SELECT 1', {}, {signal:finished.signal}),[1n]);
    assert.equal(getEventListeners(finished.signal,'abort').length,0);
    finished.abort();
    assert.deepEqual(await db.first('SELECT 2'),[2n]);
    const before = new AbortController(); before.abort();
    await assert.rejects(db.execute('INSERT INTO docs {n:66}',{}, {signal:before.signal}), e => e.code === 'FDB_CANCELLED');
    await assert.rejects(db.execute('SELECT 1',{}, {signal:{}}), TypeError);
    await db.execute('ROLLBACK');
    assert.deepEqual(await db.all('SELECT * FROM docs'),[]);
  } finally { clearTimeout(timer); await db.close(); }
});

test('AbortSignal profiling preserves transaction state and returns complete metrics on retry', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const { getEventListeners } = require('node:events');
  const db = await AsyncDatabase.open();
  let timer;
  try {
    await db.execute('CREATE TABLE numbers(n INTEGER)');
    await db.execute('INSERT INTO numbers VALUES ' + Array.from({length:100},(_,i)=>`(${i})`).join(','));
    await db.execute('BEGIN');
    await db.execute('CREATE TABLE prior');
    await db.execute('INSERT INTO prior {id:prior:a,n:9}');
    const active = new AbortController();
    const pending = db.profileSelect('SELECT count(*) FROM numbers a CROSS JOIN numbers b CROSS JOIN numbers c', {}, {signal:active.signal});
    const following = db.exactlyOne('SELECT n FROM prior');
    const settled = Promise.allSettled([pending,following]);
    timer = setTimeout(() => active.abort(),10);
    const [cancelled,success] = await settled;
    assert.equal(cancelled.status,'rejected');
    assert.equal(cancelled.reason.code,'FDB_CANCELLED');
    assert.equal(cancelled.reason.transaction.after,'active');
    assert.equal('metrics' in cancelled.reason,false);
    assert.equal(success.status,'fulfilled');
    assert.deepEqual(success.value,[9n]);
    assert.equal(getEventListeners(active.signal,'abort').length,0);
    const before = new AbortController(); before.abort();
    await assert.rejects(db.profileSelect('SELECT n FROM prior',{}, {signal:before.signal}), e => e.code === 'FDB_CANCELLED');
    const retry = new AbortController();
    const profile = await db.profileSelect('SELECT n FROM prior',{}, {signal:retry.signal});
    assert.deepEqual(profile.result.rows,[[9n]]);
    assert(profile.metrics.vmSteps>0n);
    assert.equal(profile.result.transaction.after,'active');
    assert.equal(getEventListeners(retry.signal,'abort').length,0);
    retry.abort();
    assert.deepEqual(await db.exactlyOne('SELECT n FROM prior'),[9n]);
    await db.execute('ROLLBACK');
  } finally { clearTimeout(timer); await db.close(); }
});

test('AbortSignal integrity audits preserve indexed data and allow exact retry', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const { getEventListeners } = require('node:events');
  const db = await AsyncDatabase.open();
  let timer;
  try {
    await db.execute('CREATE TABLE numbers(n INTEGER)');
    await db.execute('INSERT INTO numbers VALUES ' + Array.from({length:100},(_,i)=>`(${i})`).join(','));
    await db.execute('CREATE TABLE docs');
    await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
    await db.execute('INSERT INTO docs(n) SELECT a.n*10+b.n FROM numbers a CROSS JOIN numbers b WHERE b.n<10');
    await db.execute('BEGIN');
    await db.execute('INSERT INTO docs {id:docs:prior,n:1000}');
    const controller = new AbortController();
    const audit = db.checkCollectionIntegrity('docs',{}, {signal:controller.signal});
    const following = db.exactlyOne('SELECT n FROM docs WHERE id=docs:prior');
    const settled = Promise.allSettled([audit,following]);
    timer = setTimeout(() => controller.abort(),1);
    const [cancelled,read] = await settled;
    assert.equal(cancelled.status,'rejected');
    assert.equal(cancelled.reason.code,'FDB_CANCELLED');
    assert.equal(cancelled.reason.transaction.after,'active');
    assert.equal(read.status,'fulfilled');
    assert.deepEqual(read.value,[1000n]);
    assert.equal(getEventListeners(controller.signal,'abort').length,0);
    const before = new AbortController(); before.abort();
    await assert.rejects(db.checkCollectionIntegrity('docs',{}, {signal:before.signal}), e => e.code === 'FDB_CANCELLED');
    const retry = new AbortController();
    const complete = await db.checkCollectionIntegrity('docs',{}, {signal:retry.signal});
    assert.equal(complete.documents,1001n);
    assert.equal(complete.indexEntries,1001n);
    assert.equal(complete.transaction.after,'active');
    assert.equal(getEventListeners(retry.signal,'abort').length,0);
    retry.abort();
    await db.execute('ROLLBACK');
    assert.deepEqual(await db.exactlyOne('SELECT count(*) FROM docs'),[1000n]);
  } finally { clearTimeout(timer); await db.close(); }
});

test('AbortSignal batches retain completed reports and stop before later statements', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const { getEventListeners } = require('node:events');
  const db = await AsyncDatabase.open();
  let timer;
  try {
    await db.execute('CREATE TABLE numbers(n INTEGER)');
    await db.execute('INSERT INTO numbers VALUES ' + Array.from({length:100},(_,i)=>`(${i})`).join(','));
    await db.execute('CREATE TABLE docs');
    await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
    await db.execute('BEGIN');
    const prefix = '-- ไทย\nINSERT INTO docs {id:docs:prior,n:9}; ';
    const script = prefix + 'INSERT INTO docs(n) SELECT a.n*10000+b.n*100+c.n FROM numbers a CROSS JOIN numbers b CROSS JOIN numbers c; DELETE FROM docs; COMMIT;';
    const controller = new AbortController();
    const batch = db.executeBatch(script,{signal:controller.signal});
    const following = db.exactlyOne('SELECT n FROM docs WHERE id=docs:prior');
    timer = setTimeout(() => controller.abort(),50);
    const [reports,read] = await Promise.all([batch,following]);
    assert.equal(reports.length,2);
    assert.equal(reports[0].result.affected,1n);
    assert.equal(reports[1].offset,Buffer.byteLength(prefix));
    assert.equal(reports[1].error.code,'FDB_CANCELLED');
    assert.equal(reports[1].transaction.before,'active');
    assert.equal(reports[1].transaction.after,'active');
    assert.deepEqual(read,[9n]);
    assert.equal((await db.checkCollectionIntegrity('docs')).documents,1n);
    assert.equal(getEventListeners(controller.signal,'abort').length,0);
    const before = new AbortController(); before.abort();
    await assert.rejects(db.executeBatch("SELECT '",{signal:before.signal}),e=>e.code==='FDB_CANCELLED');
    const fresh = new AbortController();
    const retry = await db.executeBatch('INSERT INTO docs {id:docs:retry,n:10}; SELECT n FROM docs ORDER BY n;', {signal:fresh.signal});
    assert.deepEqual(retry[1].result.rows,[[9n],[10n]]);
    assert.equal(getEventListeners(fresh.signal,'abort').length,0);
    fresh.abort();
    await db.execute('ROLLBACK');
    assert.deepEqual(await db.exactlyOne('SELECT count(*) FROM docs'),[0n]);
  } finally { clearTimeout(timer); await db.close(); }
});

test('AbortSignal transfers preserve atomic imports and return complete exports on retry', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const { getEventListeners } = require('node:events');
  const source = new Database();
  try {
    source.execute('CREATE TABLE docs');
    source.execute('INSERT INTO docs(n) VALUES ' + Array.from({length:1000},(_,i)=>`(${i})`).join(','));
    for (const format of ['json','ndjson']) {
      const payload = source.exportDocuments('docs',format);
      const db = await AsyncDatabase.open();
      let timer;
      try {
        await db.execute('CREATE TABLE docs');
        await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
        await db.execute('BEGIN');
        await db.execute('INSERT INTO docs {id:docs:prior,n:1000}');
        const controller = new AbortController();
        const importing = db.importDocuments('docs',payload,format,{signal:controller.signal});
        const following = db.exactlyOne('SELECT n FROM docs WHERE id=docs:prior');
        const results = Promise.allSettled([importing,following]);
        timer = setTimeout(()=>controller.abort(),20);
        const [cancelled,read] = await results;
        assert.equal(cancelled.status,'rejected');
        assert.equal(cancelled.reason.code,'FDB_CANCELLED');
        assert.equal(cancelled.reason.transaction.after,'active');
        assert.deepEqual(read.value,[1000n]);
        assert.equal((await db.checkCollectionIntegrity('docs')).documents,1n);
        assert.equal(getEventListeners(controller.signal,'abort').length,0);
        const before = new AbortController(); before.abort();
        await assert.rejects(db.importDocuments('docs','invalid',format,{signal:before.signal}),e=>e.code==='FDB_CANCELLED');
        await assert.rejects(db.exportDocuments('docs',format,{signal:before.signal}),e=>e.code==='FDB_CANCELLED');
        const fresh = new AbortController();
        const imported = await db.importDocuments('docs',payload,format,{signal:fresh.signal});
        assert.equal(imported.imported,1000);
        assert.equal(getEventListeners(fresh.signal,'abort').length,0);
        const exporting = new AbortController();
        const output = db.exportDocuments('docs',format,{signal:exporting.signal});
        const rejected = assert.rejects(output,e=>e.code==='FDB_CANCELLED' && e.transaction.after==='active');
        timer = setTimeout(()=>exporting.abort(),1);
        await rejected;
        assert.equal(getEventListeners(exporting.signal,'abort').length,0);
        const complete = await db.exportDocuments('docs',format,{signal:fresh.signal});
        assert.equal(complete,await db.exportDocuments('docs',format));
        assert.equal((await db.checkCollectionIntegrity('docs')).documents,1001n);
        fresh.abort();
        await db.execute('ROLLBACK');
        assert.deepEqual(await db.exactlyOne('SELECT count(*) FROM docs'),[0n]);
      } finally { clearTimeout(timer); await db.close(); }
    }
  } finally { source.close(); }
});

test('AbortSignal migrations roll back all pending schema, data and history', async () => {
  const { AsyncDatabase, isFastDBError } = require('./index.cjs');
  const { getEventListeners } = require('node:events');
  const db = await AsyncDatabase.open();
  let timer;
  try {
    const base = {version:1n,name:'base',sql:'CREATE TABLE docs; INSERT INTO docs {id:docs:prior,n:9}; CREATE TABLE numbers(n INTEGER);'};
    await db.migrate([base]);
    await db.execute('INSERT INTO numbers VALUES ' + Array.from({length:100},(_,i)=>`(${i})`).join(','));
    const intermediate = {version:2n,name:'intermediate',sql:'CREATE TABLE staged; INSERT INTO staged {id:staged:first};'};
    const pending = {version:3n,name:'pending',sql:'CREATE TABLE pending; CREATE UNIQUE INDEX pending_n ON pending(n); INSERT INTO pending(n) SELECT a.n*10000+b.n*100+c.n FROM numbers a CROSS JOIN numbers b CROSS JOIN numbers c;'};
    const controller = new AbortController();
    const operation = db.migrate([base,intermediate,pending],{signal:controller.signal});
    const rejected = assert.rejects(operation,e=>{
      assert(isFastDBError(e));
      assert.equal(e.code,'FDB_CANCELLED');
      assert.equal(e.transaction.after,'autocommit');
      assert.equal(e.migration.version,pending.version);
      assert.equal(e.migration.offset,BigInt(Buffer.byteLength(pending.sql.slice(0,pending.sql.indexOf('INSERT INTO')))));
      assert.equal(e.migration.cause.code,'FDB_CANCELLED');
      return true;
    });
    timer=setTimeout(()=>controller.abort(),100);
    await rejected;
    assert.equal(getEventListeners(controller.signal,'abort').length,0);
    assert.deepEqual(await db.exactlyOne('SELECT n FROM docs'),[9n]);
    await assert.rejects(db.execute('SELECT * FROM pending'));
    await assert.rejects(db.execute('SELECT * FROM staged'));
    assert.equal((await db.migrate([base])).alreadyApplied,1);
    const before = new AbortController(); before.abort();
    await assert.rejects(db.migrate([base,intermediate,pending],{signal:before.signal}),e=>{
      assert(isFastDBError(e));
      assert.equal(e.code,'FDB_CANCELLED');
      assert.equal(e.migration,undefined);
      return true;
    });
    const fresh = new AbortController();
    const retry = {...pending,sql:'CREATE TABLE pending; CREATE UNIQUE INDEX pending_n ON pending(n); INSERT INTO pending(n) VALUES (1),(2);'};
    assert.deepEqual((await db.migrate([base,intermediate,retry],{signal:fresh.signal})).applied,[2n,3n]);
    assert.equal((await db.migrate([base,intermediate,retry])).alreadyApplied,3);
    assert.equal((await db.checkCollectionIntegrity('pending')).documents,2n);
    assert.equal(getEventListeners(fresh.signal,'abort').length,0);
    fresh.abort();
    assert.deepEqual(await db.exactlyOne('SELECT n FROM docs'),[9n]);
  } finally {clearTimeout(timer); await db.close();}
});

test('close drains cancelled operations and rolls back the remaining outer transaction', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const { getEventListeners } = require('node:events');
  const dir = fs.mkdtempSync(path.join(os.tmpdir(),'fastdb-cancel-close-'));
  const file = path.join(dir,'test.db');
  const db = await AsyncDatabase.open(file);
  let timer;
  try {
    await db.execute('CREATE TABLE numbers(n INTEGER)');
    await db.execute('INSERT INTO numbers VALUES ' + Array.from({length:100},(_,i)=>`(${i})`).join(','));
    await db.execute('CREATE TABLE docs');
    await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
    await db.execute('INSERT INTO docs {id:docs:committed,n:7}');
    await db.execute('BEGIN');
    await db.execute('INSERT INTO docs {id:docs:prior,n:9}');
    const active = new AbortController();
    const queued = new AbortController();
    const tasks = [
      db.execute('INSERT INTO docs(n) SELECT a.n*10000+b.n*100+c.n FROM numbers a CROSS JOIN numbers b CROSS JOIN numbers c',{}, {signal:active.signal}),
      db.executeBatch('DELETE FROM docs;', {signal:queued.signal}),
      db.profileSelect('SELECT * FROM docs',{}, {signal:queued.signal}),
      db.checkCollectionIntegrity('docs',{}, {signal:queued.signal}),
      db.importDocuments('docs','invalid','json',{signal:queued.signal}),
      db.exportDocuments('docs','ndjson',{signal:queued.signal}),
      db.migrate([], {signal:queued.signal}),
    ];
    const settled = Promise.allSettled(tasks);
    queued.abort();
    const closing = db.close();
    assert.equal(db.close(),closing);
    await assert.rejects(db.execute('SELECT 1'),/closing or closed/);
    timer = setTimeout(()=>active.abort(),20);
    const results = await settled;
    for (const result of results) {
      assert.equal(result.status,'rejected');
      assert.equal(result.reason.code,'FDB_CANCELLED');
      assert.equal(result.reason.transaction.after,'active');
    }
    await closing;
    assert.equal(db.interrupt(),false);
    assert.equal(getEventListeners(active.signal,'abort').length,0);
    assert.equal(getEventListeners(queued.signal,'abort').length,0);
    const reopened = new Database(file);
    try {
      assert.deepEqual(reopened.exactlyOne('SELECT n FROM docs'),[7n]);
      assert.equal(reopened.checkCollectionIntegrity('docs').documents,1n);
    } finally { reopened.close(); }
  } finally { clearTimeout(timer); await db.close(); fs.rmSync(dir,{recursive:true,force:true}); }
});

test('native membership compound queries prepare in sync and worker clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [()=>new Database(),()=>AsyncDatabase.open()]) {
    const db=await open();
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('INSERT INTO docs(n) VALUES (1),(2),(NULL)');
      await db.execute('CREATE TABLE rhs(n INTEGER)');
      await db.execute('INSERT INTO rhs VALUES (1),(NULL)');
      assert.deepEqual(await db.all('SELECT n IN (SELECT n FROM rhs) FROM docs UNION ALL SELECT n NOT IN (SELECT n FROM rhs) FROM docs'),[[1n],[null],[null],[0n],[null],[null]]);
      assert.deepEqual(await db.all('WITH r AS (SELECT n FROM rhs) SELECT n IN (SELECT n FROM r) FROM docs'),[[1n],[null],[null]]);
    } finally { await db.close(); }
  }
});

test('profiles expose separate forward target counters in both clients', async () => {
  const {AsyncDatabase}=require('./index.cjs');
  for (const db of [new Database(), await AsyncDatabase.open()]) {
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('INSERT INTO docs {id:docs:a,n:1}');
      const sql='SELECT record::fetch(docs:a) AS a,record::fetch(docs:a) AS b';
      const profile=await db.profileSelect(sql);
      assert.equal(profile.metrics.fetchBatches,1n);
      assert(profile.metrics.fetchRowsRead>=1n);
      assert(profile.metrics.fetchVmSteps>0n);
      assert.deepEqual(profile.result.rows[0][0],profile.result.rows[0][1]);
      assert.deepEqual((await db.profileSelect(sql)).metrics,profile.metrics);
      const plain=await db.profileSelect('SELECT 1');
      assert.equal(plain.metrics.fetchBatches,0n);
      assert.equal(plain.metrics.fetchRowsRead,0n);
      assert.equal(plain.metrics.fetchVmSteps,0n);
    } finally {await db.close();}
  }
});

test('leading WITH updates and deletes work in both clients', async () => {
  const {AsyncDatabase}=require('./index.cjs');
  for (const db of [new Database(),await AsyncDatabase.open()]) {
    try {
      await db.execute('CREATE TABLE docs');
      const cte = 'WITH docs AS (SELECT 2 AS n), chosen AS (SELECT docs.n FROM docs) SELECT chosen.n FROM chosen';
      assert.deepEqual(await db.all(cte), [[2n]]);
      assert.deepEqual((await db.profileSelect(cte)).result.rows, [[2n]]);
      await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
      await db.execute('INSERT INTO docs(n) VALUES(1),(2),(3)');
      await db.execute('BEGIN');
      const updated=await db.execute('WITH chosen AS (SELECT $n AS n) UPDATE docs SET n=(SELECT n FROM chosen)+10 WHERE n IN (SELECT n FROM chosen) RETURNING n',{$n:2n});
      assert.deepEqual(updated.rows,[[12n]]);
      assert.equal(updated.affected,1n);
      const deleted=await db.execute('WITH chosen AS (SELECT n FROM docs WHERE n>$min) DELETE FROM docs WHERE n IN (SELECT n FROM chosen) RETURNING n',{$min:10n});
      assert.deepEqual(deleted.rows,[[12n]]);
      assert.equal((await db.checkCollectionIntegrity('docs')).documents,2n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'),[[1n],[2n],[3n]]);
    } finally {await db.close();}
  }
});


test('native scalar predicates correlate in sync and worker clients', async () => {
  const {AsyncDatabase}=require('./index.cjs');
  for (const db of [new Database(),await AsyncDatabase.open()]) {
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
      await db.execute('INSERT INTO docs(n) VALUES(1),(2),(3)');
      await db.execute('CREATE TABLE lookup(n INTEGER)');
      await db.execute('INSERT INTO lookup VALUES(1),(2),(3)');
      const sql='SELECT d.n,(SELECT max(n) FROM lookup WHERE n<d.n) AS prior FROM docs AS d ORDER BY d.n';
      assert.deepEqual(await db.all(sql),[[1n,null],[2n,1n],[3n,2n]]);
      assert.deepEqual((await db.profileSelect(sql)).result.rows,[[1n,null],[2n,1n],[3n,2n]]);
      const grouped='SELECT d.n,(SELECT max(n) AS maximum FROM lookup HAVING maximum>d.n) FROM docs AS d ORDER BY d.n';
      assert.deepEqual(await db.all(grouped),[[1n,3n],[2n,3n],[3n,null]]);
      assert.deepEqual((await db.profileSelect(grouped)).result.rows,[[1n,3n],[2n,3n],[3n,null]]);
      const membership='SELECT d.n,d.n IN(SELECT n FROM lookup WHERE n=d.n AND n<3) FROM docs AS d ORDER BY d.n';
      assert.deepEqual(await db.all(membership),[[1n,1n],[2n,1n],[3n,0n]]);
      assert.deepEqual((await db.profileSelect(membership)).result.rows,[[1n,1n],[2n,1n],[3n,0n]]);
      await db.execute('BEGIN');
      const result=await db.execute('UPDATE docs AS d SET n=(SELECT max(n) FROM lookup WHERE n<=d.n)+$delta RETURNING n',{$delta:10n});
      assert.equal(result.affected,3n);
      assert.deepEqual(result.rows,[[11n],[12n],[13n]]);
      assert.equal((await db.checkCollectionIntegrity('docs')).documents,3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'),[[1n],[2n],[3n]]);
    } finally {await db.close();}
  }
});


test('async startup failure permits repeated opens and a healthy retry', {timeout:10000}, async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const fs = require('node:fs');
  const os = require('node:os');
  const path = require('node:path');
  const directory = fs.mkdtempSync(path.join(os.tmpdir(),'fastdb-open-failure-'));
  try {
    for (let attempt=0; attempt<3; attempt++) {
      await assert.rejects(AsyncDatabase.open(path.join(directory,'missing','database.db')), error=>error.code==='FDB_WORKER');
    }
    const file=path.join(directory,'database.db');
    const db=await AsyncDatabase.open(file);
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('INSERT INTO docs {id:docs:saved,n:7}');
    } finally { await db.close(); }
    const reopened=await AsyncDatabase.open(file);
    try { assert.deepEqual(await reopened.exactlyOne('SELECT n FROM docs'),[7n]); }
    finally { await reopened.close(); }
  } finally { fs.rmSync(directory,{recursive:true,force:true}); }
});


test('closed sync and worker handles expose FDB_CLOSED across operations', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const operations = [
    db=>db.execute('SELECT 1'), db=>db.all('SELECT 1'), db=>db.first('SELECT 1'), db=>db.exactlyOne('SELECT 1'),
    db=>db.profileSelect('SELECT 1'), db=>db.executeBatch('SELECT 1;'),
    db=>db.checkCollectionIntegrity('docs'), db=>db.exportDocuments('docs'),
    db=>db.importDocuments('docs','{}'), db=>db.migrate([]),
  ];
  const closed = error => error.code === 'FDB_CLOSED' && !Object.hasOwn(error,'transaction');
  const sync = new Database();
  sync.close(); sync.close();
  for (const operation of operations) assert.throws(()=>operation(sync),closed);
  const worker = await AsyncDatabase.open();
  const closing=worker.close();
  assert.equal(worker.close(),closing);
  await closing;
  for (const operation of operations) await assert.rejects(operation(worker),closed);
});


test('exactlyOne cardinality errors retain completed statement transaction observations', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const db of [new Database(), await AsyncDatabase.open()]) {
    const reject = (sql, state) => assert.rejects(async()=>db.exactlyOne(sql), error=>
      error instanceof RangeError && error.code==='FDB_CARDINALITY' &&
      error.transaction.before===state && error.transaction.after===state);
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
      await reject('SELECT n FROM docs','autocommit');
      await reject('INSERT INTO docs(n) VALUES(1),(2) RETURNING n','autocommit');
      assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'),[[1n],[2n]]);
      await db.execute('BEGIN');
      await reject('UPDATE docs SET n=n+10 RETURNING n','active');
      assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'),[[11n],[12n]]);
      assert.equal((await db.checkCollectionIntegrity('docs')).indexEntries,2n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'),[[1n],[2n]]);
      assert.deepEqual(await db.exactlyOne('SELECT n FROM docs WHERE n=1'),[1n]);
    } finally { await db.close(); }
  }
});

test('exactlyOne preserves execution errors and worker cancellation', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const db of [new Database(), await AsyncDatabase.open()]) {
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
      await db.execute('INSERT INTO docs {n:1}');
      await db.execute('BEGIN');
      await assert.rejects(async()=>db.exactlyOne('INSERT INTO docs {n:1} RETURNING n'), error=>
        error.code === 'FDB_CONSTRAINT' && error.transaction.before==='active' && error.transaction.after==='active');
      assert.deepEqual(await db.exactlyOne('SELECT count(*) FROM docs'),[1n]);
      if (db instanceof AsyncDatabase) {
        const controller=new AbortController(); controller.abort();
        await assert.rejects(db.exactlyOne('SELECT n FROM docs',{}, {signal:controller.signal}), error=>
          error.code==='FDB_CANCELLED' && error.transaction.before==='active' && error.transaction.after==='active');
        assert.deepEqual(await db.exactlyOne('SELECT n FROM docs'),[1n]);
      }
      await db.execute('ROLLBACK');
      assert.equal((await db.checkCollectionIntegrity('docs')).indexEntries,1n);
    } finally { await db.close(); }
  }
});


test('isFastDBError recognizes public errors and validates transaction observations', async () => {
  const { AsyncDatabase, isFastDBError } = require('./index.cjs');
  for (const value of [null,undefined,{},new Error('ordinary'),{code:'FDB_ENGINE'},Object.assign(new Error(),{code:'FDB_'}),Object.assign(new Error(),{code:'FDB_ENGINE',transaction:null}),Object.assign(new Error(),{code:'FDB_ENGINE',transaction:{before:'active',after:'committed'}})]) {
    assert.equal(isFastDBError(value),false);
  }
  for (const open of [()=>new Database(), ()=>AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE docs');
      await assert.rejects(async()=>db.execute('INSERT INTO docs {n:$missing}'), error=>isFastDBError(error) && error.code==='FDB_PARAMETER');
      await assert.rejects(async()=>db.exactlyOne('SELECT 1 WHERE 0'), error=>isFastDBError(error) && error.code==='FDB_CARDINALITY' && error.transaction.after==='autocommit');
    } finally { await db.close(); }
    await assert.rejects(async()=>db.all('SELECT 1'), error=>isFastDBError(error) && error.code==='FDB_CLOSED' && error.transaction===undefined);
  }
});

test('migration diagnostics preserve UTF-8 locations and history reasons in both clients', async () => {
  const { AsyncDatabase, isFastDBError } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      const sql = "-- café 日本語\nSELECT 'unterminated";
      const offset = Buffer.byteLength(sql.slice(0, sql.indexOf("'")));
      const plan = [
        {version:1n,name:'base.sql',sql:'CREATE TABLE docs;'},
        {version:9007199254740993n,name:'pending.sql',sql},
      ];
      await assert.rejects(async () => db.migrate(plan), error => {
        assert(isFastDBError(error));
        assert.equal(error.code,'FDB_SYNTAX');
        assert(error.message.includes(`byte ${offset}:`),error.message);
        assert(error.message.includes('migration 9007199254740993:'),error.message);
        assert.deepEqual(error.transaction,{before:'autocommit',after:'autocommit'});
        return true;
      });
      await assert.rejects(async () => db.all('SELECT * FROM docs'));
      plan[1].sql = 'SELECT 1;';
      assert.deepEqual((await db.migrate(plan)).applied,[1n,9007199254740993n]);
      for (const [field,value,reason] of [
        ['name','renamed.sql','name differs'],
        ['sql','SELECT 1; -- edited','SQL source differs'],
        ['version',9007199254740994n,'version does not match the applied sequence'],
      ]) {
        const changed = [plan[0],{...plan[1],[field]:value}];
        await assert.rejects(async () => db.migrate(changed),error => {
          assert.equal(error.code,'FDB_VALIDATION');
          assert(error.message.includes(reason),error.message);
          assert.deepEqual(error.transaction,{before:'autocommit',after:'autocommit'});
          return true;
        });
      }
      assert.equal((await db.migrate(plan)).alreadyApplied,2);
    } finally { await db.close(); }
  }
});

test('composite count preserves null presence through both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE docs');
      for (const [n,v] of [[1n,[]],[2n,{a:1n}],[3n,new Record('users','one')],[4n,Buffer.from([0,1])],[5n,null],[6n,false]]) {
        await db.execute('INSERT INTO docs(n,v) VALUES($n,$v)',{$n:n,$v:v});
      }
      await db.execute('INSERT INTO docs {n:7}');
      const sql='SELECT count(v),count(missing),count(*) FROM docs';
      assert.deepEqual(await db.exactlyOne(sql),[5n,0n,7n]);
      assert.deepEqual((await db.profileSelect(sql)).result.rows,[[5n,0n,7n]]);
      for (const value of [[],{},new Record('users','two'),Buffer.alloc(0),false,null]) {
        assert.deepEqual(await db.exactlyOne('SELECT count($value) FROM docs',{$value:value}),[value===null?0n:7n]);
      }
      assert.deepEqual(await db.all('SELECT count(v) OVER (ORDER BY n) FROM docs ORDER BY n'),[[1n],[2n],[3n],[4n],[4n],[5n],[5n]]);
    } finally { await db.close(); }
  }
});

test('unnamed derived joins preserve client values and ambiguity errors', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE docs');
      const bytes = Buffer.from([70,68,66,1,0,255]);
      await db.execute('INSERT INTO docs {n:1,flag:true,link:docs:b,data:$data}', {$data:bytes});
      await db.execute('CREATE TABLE labels(m INTEGER,label TEXT)');
      await db.execute("INSERT INTO labels VALUES(1,'A')");
      const sql = 'SELECT n,flag,link,data,label FROM (SELECT n,flag,link,data FROM docs) JOIN labels ON n=m';
      const expected = [1n,true,new Record('docs','b'),bytes,'A'];
      assert.deepEqual(await db.exactlyOne(sql),expected);
      assert.deepEqual((await db.profileSelect(sql)).result.rows,[expected]);
      await db.execute('ALTER TABLE labels ADD COLUMN n INTEGER');
      await assert.rejects(async () => db.execute(sql), {code:'FDB_VALIDATION'});
      assert.deepEqual(await db.exactlyOne('SELECT d.n,flag,link,data,label FROM (SELECT n,flag,link,data FROM docs) d JOIN labels l ON d.n=l.m'),expected);
    } finally { await db.close(); }
  }
});

test('JOIN membership preserves binary bindings and NULL rows in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      const bytes = Buffer.from([70, 68, 66, 1, 0, 255]);
      const other = Buffer.from([49]);
      await db.execute('CREATE TABLE docs');
      await db.execute('INSERT INTO docs {n:1,data:$data}', {$data:bytes});
      await db.execute('INSERT INTO docs {n:2,data:$data}', {$data:other});
      await db.execute('CREATE TABLE labels(v BLOB)');
      await db.execute('INSERT INTO labels VALUES($value)', {$value:bytes});
      await db.execute('CREATE TABLE marker(m INTEGER)');
      await db.execute('INSERT INTO marker VALUES(7)');
      for (const operator of ['IN', 'NOT IN']) {
        const sql = `SELECT d.n,d.data,marker.m FROM (SELECT n,data FROM docs) d LEFT JOIN marker ON d.data ${operator} (SELECT v FROM labels WHERE v=$value) ORDER BY d.n`;
        const expected = operator === 'IN'
          ? [[1n, bytes, 7n], [2n, other, null]]
          : [[1n, bytes, null], [2n, other, 7n]];
        assert.deepEqual((await db.execute(sql, {$value:bytes})).rows, expected);
        assert.deepEqual((await db.profileSelect(sql, {$value:bytes})).result.rows, expected);
      }
    } finally { await db.close(); }
  }
});


test('native loader enforces the declared Node minimum before loading', () => {
  const vm = require('node:vm');
  const source = fs.readFileSync(path.join(__dirname, 'native.cjs'), 'utf8');
  const declared = require('./package.json').engines.node;
  assert.equal(declared, '>=22');
  for (const version of ['18.20.0', '20.19.0', '21.7.0', '22.0.0', '24.19.0']) {
    let loads = 0;
    const addon = {};
    const context = {
      process: { versions: { node: version }, platform: 'linux', arch: 'x64' },
      module: { exports: {} },
      require(name) { assert.equal(name, './fastdb.node'); loads++; return addon; },
    };
    if (Number.parseInt(version, 10) < 22) {
      assert.throws(() => vm.runInNewContext(source, context), error => {
        assert.equal(error.code, 'FDB_RUNTIME_VERSION');
        assert(error.message.includes(version));
        assert(error.message.includes('22 or newer'));
        return true;
      });
      assert.equal(loads, 0);
    } else {
      vm.runInNewContext(source, context);
      assert.equal(loads, 1);
      assert.equal(context.module.exports, addon);
    }
  }
});

test('duplicate projection names preserve positional values in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('INSERT INTO docs {n:1,flag:true,data:$data}', { $data: Buffer.from([49]) });
      const result = await db.execute('SELECT flag AS x,data AS x FROM docs');
      assert.deepEqual(result.columns, ['x', 'x']);
      assert.deepEqual(result.rows, [[true, Buffer.from([49])]]);
      const nested = await db.execute('SELECT q.* FROM (SELECT flag AS x,data AS x FROM docs) q');
      assert.deepEqual(nested.columns, result.columns);
      assert.deepEqual(nested.rows, result.rows);
      const cte = await db.execute('WITH q(x,x) AS (SELECT flag,data FROM docs), r AS (SELECT q.* FROM q) SELECT r.* FROM r');
      assert.deepEqual(cte.columns, result.columns);
      assert.deepEqual(cte.rows, result.rows);
      const foldedCte = await db.execute('WITH q(x,X) AS (SELECT flag,data FROM docs), r AS (SELECT q.* FROM q) SELECT r.* FROM r');
      assert.deepEqual(foldedCte.columns, ['x', 'x']);
      assert.deepEqual(foldedCte.rows, result.rows);
      const inheritedNames = await db.execute('WITH q AS (SELECT flag AS x,data AS X FROM docs) SELECT q.* FROM q');
      assert.deepEqual(inheritedNames.columns, ['x', 'X']);
      assert.deepEqual(inheritedNames.rows, result.rows);
      const declaredName = await db.execute('WITH q("Flag") AS (SELECT flag FROM docs) SELECT q."FLAG",q."Flag" AS "Public Name" FROM q');
      assert.deepEqual(declaredName.columns, ['flag', 'Public Name']);
      assert.deepEqual(declaredName.rows, [[true, true]]);
      const star = await db.execute('SELECT q.* FROM docs d JOIN (SELECT 10 AS x,20 AS x) q ON 1');
      assert.deepEqual(star.columns, ['x', 'x']);
      assert.deepEqual(star.rows, [[10n, 20n]]);
      const nativeCte = await db.execute('WITH q(x,x) AS (SELECT 10,20) SELECT v.* FROM docs d JOIN q v ON 1');
      assert.deepEqual(nativeCte.columns, star.columns);
      assert.deepEqual(nativeCte.rows, star.rows);
      const named = await db.execute('WITH q(x,x) AS (SELECT 10,20) SELECT v.X FROM docs d JOIN q v ON 1');
      assert.deepEqual(named.columns, ['x']);
      assert.deepEqual(named.rows, [[10n]]);
      const using = await db.execute('SELECT * FROM (SELECT n FROM docs) a JOIN (SELECT 1 AS n,2 AS extra) b USING(n)');
      assert.deepEqual(using.columns, ['n','extra']);
      assert.deepEqual(using.rows, [[1n,2n]]);
      const natural = await db.execute('SELECT * FROM (SELECT n FROM docs) a NATURAL JOIN (SELECT 1 AS n,2 AS extra) b');
      assert.deepEqual(natural.columns, using.columns);
      assert.deepEqual(natural.rows, using.rows);
      const correlatedUsing = await db.execute('SELECT n,(SELECT n) AS value FROM (SELECT n FROM docs) a RIGHT JOIN (SELECT 1 AS n UNION ALL SELECT 2) b USING(n) ORDER BY n');
      assert.deepEqual(correlatedUsing.rows, [[1n,1n],[2n,2n]]);
      const nestedUsing = await db.execute('SELECT n,(SELECT (SELECT n)) AS value FROM (SELECT n FROM docs) a RIGHT JOIN (SELECT 1 AS n UNION ALL SELECT 2) b USING(n) ORDER BY n');
      assert.deepEqual(nestedUsing.rows, correlatedUsing.rows);
      await db.execute('CREATE TABLE nums(value INTEGER)');
      await db.execute('INSERT INTO nums VALUES(0),(1)');
      for (const [join, constraint] of [['RIGHT JOIN', ' USING(n)'], ['NATURAL RIGHT JOIN', '']]) {
        const tableScalar = await db.execute(`SELECT n,(SELECT (SELECT n FROM nums WHERE value=1)) AS value FROM (SELECT n FROM docs) a ${join} (SELECT 1 AS n UNION ALL SELECT 2) b${constraint} ORDER BY n`);
        assert.deepEqual(tableScalar.columns, ['n', 'value']);
        assert.deepEqual(tableScalar.rows, correlatedUsing.rows);
      }
      for (const field of ['flag', 'data']) {
        const typedScalar = await db.execute(`SELECT k,(SELECT (SELECT k FROM nums ORDER BY value DESC LIMIT 1)) AS value FROM (SELECT ${field} AS k FROM docs) a JOIN (SELECT ${field} AS k FROM docs) b USING(k)`);
        const expected = field === 'flag' ? true : Buffer.from([49]);
        assert.deepEqual(typedScalar.rows, [[expected, expected]]);
      }
    } finally { await db.close(); }
  }
});

test('nested USING runtime errors report outer rollback in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of ['CREATE TABLE docs', 'INSERT INTO docs {k:1,v:[]}', 'INSERT INTO docs {k:2,v:7}', 'CREATE TABLE b(k INTEGER)', 'INSERT INTO b VALUES(1),(2)', 'CREATE TABLE copied', 'INSERT INTO copied {k:99,v:[]}']) await db.execute(sql);
      const insert = 'INSERT INTO copied(k,v) SELECT k,(SELECT (SELECT array::append(a.v,2))) FROM docs a JOIN b USING(k) ORDER BY k';
      await db.execute('BEGIN');
      await db.execute('INSERT INTO copied {k:98,v:[]}');
      await assert.rejects(async () => db.execute(insert), error => {
        assert.deepEqual(error.transaction, { before: 'active', after: 'autocommit' });
        return true;
      });
      assert.deepEqual((await db.execute('SELECT k FROM copied ORDER BY k')).rows, [[99n]]);
      await db.execute('BEGIN');
      await db.execute('UPDATE docs SET v=array::new() WHERE k=2');
      await db.execute(insert);
      assert.deepEqual((await db.execute('SELECT k,v FROM copied WHERE k<>99 ORDER BY k')).rows, [[1n,[2n]],[2n,[2n]]]);
      await db.execute('ROLLBACK');
      assert.deepEqual((await db.execute('SELECT k FROM copied ORDER BY k')).rows, [[99n]]);
    } finally { await db.close(); }
  }
});

test('nested membership CTE writes preserve recovery in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
      await db.execute('INSERT INTO docs(n) VALUES(1),(2),(3)');
      for (const prefix of [
        'WITH docs AS (SELECT 2 AS n), chosen AS (SELECT n FROM (SELECT n FROM docs) q WHERE n IN (SELECT x.n FROM docs x)) ',
        'WITH docs AS (SELECT 2 AS n), chosen AS (WITH local_q AS MATERIALIZED (SELECT n FROM docs) SELECT n FROM local_q) ',
        'WITH docs AS (SELECT 2 AS n), chosen AS (WITH docs AS (SELECT n FROM main.docs), local_q AS NOT MATERIALIZED (SELECT n FROM docs) SELECT n FROM local_q) ',
      ]) {
        for (let attempt = 0; attempt < 2; attempt++) {
          await db.execute('BEGIN');
          await db.execute('CREATE TABLE pending(n INTEGER)');
          await db.execute('INSERT INTO pending VALUES(9)');
          await assert.rejects(async () => db.execute(prefix + 'UPDATE docs SET n=1 WHERE n IN (SELECT n FROM chosen) RETURNING n'), error => error.code === 'FDB_CONSTRAINT' && error.transaction.after === 'active');
          assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'), [[1n],[2n],[3n]]);
          assert.deepEqual(await db.all('SELECT n FROM pending'), [[9n]]);
          const result = await db.execute(prefix + 'UPDATE docs SET n=n+10 WHERE n IN (SELECT n FROM chosen) RETURNING n');
          assert.equal(result.affected, 3n);
          assert.deepEqual(result.rows, [[11n],[12n],[13n]]);
          const audit = await db.checkCollectionIntegrity('docs');
          assert.equal(audit.documents, 3n);
          assert.equal(audit.indexEntries, 3n);
          await db.execute('ROLLBACK');
          assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'), [[1n],[2n],[3n]]);
        }
      }
    } finally { await db.close(); }
  }
});

test('unsupported correlated CTE writes report query errors in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('INSERT INTO docs(n) VALUES(1),(2)');
      await db.execute('BEGIN');
      await db.execute('INSERT INTO docs(n) VALUES(9)');
      const sql = 'WITH chosen AS(WITH local_q AS(SELECT d.n AS n) SELECT n FROM local_q) UPDATE docs AS d SET n=n+10 WHERE EXISTS(SELECT 1 FROM chosen) RETURNING n';
      await assert.rejects(async () => db.execute(sql), error => {
        assert.equal(error.code, 'FDB_UNSUPPORTED');
        assert.deepEqual(error.transaction, {before:'active', after:'active'});
        return true;
      });
      assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'), [[1n],[2n],[9n]]);
      assert.equal((await db.execute('UPDATE docs SET n=n+10 WHERE n=1')).affected, 1n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'), [[1n],[2n]]);
    } finally { await db.close(); }
  }
});

test('colliding scalar aliases preserve client values and atomic insert recovery', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', 'INSERT INTO docs(k) VALUES(1),(2)',
        'CREATE TABLE b(k INTEGER)', 'INSERT INTO b VALUES(1),(3)',
        'CREATE TABLE nums(n INTEGER)', 'INSERT INTO nums VALUES(0),(1),(2)',
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_v ON sink(v)',
        'INSERT INTO sink(v) VALUES(2)',
      ]) await db.execute(sql);
      const params = { $delta: 0n, $flag: true, $data: Buffer.from([0, 255]) };
      for (const alias of ['a', 'b']) {
        for (const join of ['LEFT JOIN', 'RIGHT JOIN']) {
          const scalar = `(SELECT max(${alias}.n)+$delta FROM nums ${alias} WHERE ${alias}.n<k)`;
          const result = await db.execute(`SELECT k,${scalar} AS v,$flag AS flag,$data AS data FROM docs a ${join} b USING(k) ORDER BY k`, params);
          const keys = join === 'LEFT JOIN' ? [[1n, 0n], [2n, 1n]] : [[1n, 0n], [3n, 2n]];
          assert.deepEqual(result.columns, ['k', 'v', 'flag', 'data']);
          assert.deepEqual(result.rows, keys.map(row => [...row, true, params.$data]));
        }
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(v) VALUES(9)');
      const insert = 'INSERT INTO sink(v) SELECT (SELECT max(b.n)+$delta FROM nums b WHERE b.n<k) FROM docs a RIGHT JOIN b USING(k) ORDER BY k RETURNING v';
      await assert.rejects(async () => db.execute(insert, { $delta: 0n }), error => {
        assert.equal(error.code, 'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      assert.deepEqual(await db.all('SELECT v FROM sink ORDER BY v'), [[2n], [9n]]);
      await db.execute('DELETE FROM sink WHERE v=2');
      const retry = await db.execute(insert, { $delta: 0n });
      assert.equal(retry.affected, 2n);
      assert.deepEqual(retry.rows, [[0n], [2n]]);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents, 3n);
      assert.equal(audit.indexEntries, 3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT v FROM sink'), [[2n]]);
    } finally { await db.close(); }
  }
});

test('deeper EXISTS merged keys preserve parameters and write recovery in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', 'INSERT INTO docs(k) VALUES(1),(2)',
        'CREATE TABLE native(k INTEGER)', 'INSERT INTO native VALUES(1),(2)',
        'CREATE TABLE b(k INTEGER)', 'INSERT INTO b VALUES(1),(3)',
        'CREATE TABLE nums(n INTEGER)', 'INSERT INTO nums VALUES(0),(1),(2)',
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_k ON sink(k)',
        'INSERT INTO sink(k) VALUES(3)',
      ]) await db.execute(sql);
      const scalar = reference => `(SELECT max(b.n) FROM nums b WHERE b.n<k AND EXISTS(SELECT 1 FROM (SELECT 0 AS k) q WHERE EXISTS(SELECT 1 WHERE ${reference}>$min)))`;
      for (const reference of ['k', 'q.k']) {
        for (const min of [0n, 1n, 3n]) {
          const query = source => `SELECT k,${scalar(reference)} AS v FROM ${source} a RIGHT JOIN b USING(k) ORDER BY k`;
          const expected = await db.execute(query('native'), { $min: min });
          const actual = await db.execute(query('docs'), { $min: min });
          assert.deepEqual(actual.columns, expected.columns);
          assert.deepEqual(actual.rows, expected.rows);
        }
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(k) VALUES(9)');
      const insert = `INSERT INTO sink(k,v) SELECT k,${scalar('k')} FROM docs a RIGHT JOIN b USING(k) ORDER BY k RETURNING k,v`;
      for (const [params, code] of [[{}, 'FDB_PARAMETER'], [{ $min: 1n }, 'FDB_CONSTRAINT']]) {
        await assert.rejects(async () => db.execute(insert, params), error => {
          assert.equal(error.code, code);
          assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
          return true;
        });
        assert.deepEqual(await db.all('SELECT k FROM sink ORDER BY k'), [[3n], [9n]]);
      }
      await db.execute('DELETE FROM sink WHERE k=3');
      const retry = await db.execute(insert, { $min: 1n });
      assert.equal(retry.affected, 2n);
      assert.deepEqual(retry.rows, [[1n, null], [3n, 2n]]);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents, 3n);
      assert.equal(audit.indexEntries, 3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT k FROM sink'), [[3n]]);
    } finally { await db.close(); }
  }
});

test('deeper scalar collation and casts preserve client results and atomic writes', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', "INSERT INTO docs(k) VALUES(1),(2),('a')",
        'CREATE TABLE native(k INTEGER)', "INSERT INTO native VALUES(1),(2),('a')",
        'CREATE TABLE b(k INTEGER)', "INSERT INTO b VALUES(1),(3),('a')",
        'CREATE TABLE nums(n INTEGER)', 'INSERT INTO nums VALUES(0),(1),(2)',
        'CREATE TABLE labels(v TEXT COLLATE NOCASE)', "INSERT INTO labels VALUES('A')",
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_k ON sink(k)',
        "INSERT INTO sink(k) VALUES('a')",
      ]) await db.execute(sql);
      for (const predicate of [
        '(SELECT CAST(k AS TEXT))=$value',
        '(SELECT v FROM labels LIMIT 1)=k',
        '(SELECT $value COLLATE NOCASE)=k',
      ]) {
        const params = predicate.includes('$value') ? { $value: predicate.includes('CAST') ? 3n : 'A' } : {};
        const query = source => `SELECT k,(SELECT max(x.n) FROM nums x WHERE x.n<k AND ${predicate}) AS v FROM ${source} d RIGHT JOIN b USING(k) ORDER BY k`;
        const expected = await db.execute(query('native'), params);
        const actual = await db.execute(query('docs'), params);
        assert.deepEqual(actual.columns, expected.columns);
        assert.deepEqual(actual.rows, expected.rows);
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(k) VALUES(9)');
      const insert = 'INSERT INTO sink(k,v) SELECT k,(SELECT max(x.n) FROM nums x WHERE x.n<k AND (SELECT v FROM labels LIMIT 1)=k) FROM docs d RIGHT JOIN b USING(k) ORDER BY k RETURNING k,v';
      await assert.rejects(async () => db.execute(insert), error => {
        assert.equal(error.code, 'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      assert.deepEqual(await db.all('SELECT k FROM sink ORDER BY k'), [[9n], ['a']]);
      await db.execute("DELETE FROM sink WHERE k='a'");
      const retry = await db.execute(insert);
      assert.equal(retry.affected, 3n);
      assert.deepEqual(retry.rows, [[1n, null], [3n, null], ['a', 2n]]);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents, 4n);
      assert.equal(audit.indexEntries, 4n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT k FROM sink'), [['a']]);
    } finally { await db.close(); }
  }
});

test('deeper membership preserves nulls and atomic writes in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', 'INSERT INTO docs(k) VALUES(1),(2)',
        'CREATE TABLE native(k INTEGER)', 'INSERT INTO native VALUES(1),(2)',
        'CREATE TABLE b(k INTEGER)', 'INSERT INTO b VALUES(1),(3)',
        'CREATE TABLE nums(n INTEGER)', 'INSERT INTO nums VALUES(0),(1),(2),(NULL)',
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_k ON sink(k)',
        'INSERT INTO sink(k) VALUES(3)',
      ]) await db.execute(sql);
      const select = (source, rhs) => `SELECT k,(SELECT sum(CASE WHEN x.n IN(${rhs}) THEN 1 WHEN x.n NOT IN(${rhs}) THEN 10 ELSE 100 END) FROM nums x WHERE k IS k) AS v FROM ${source} d RIGHT JOIN b USING(k) ORDER BY k`;
      for (const rhs of ['SELECT k', 'SELECT k WHERE 0', 'SELECT $value', 'SELECT x.n']) {
        for (const value of rhs.includes('$value') ? [null, 1n] : [undefined]) {
          const params = value === undefined ? {} : { $value: value };
          const expected = await db.execute(select('native', rhs), params);
          const actual = await db.execute(select('docs', rhs), params);
          assert.deepEqual(actual.columns, expected.columns);
          assert.deepEqual(actual.rows, expected.rows);
        }
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(k) VALUES(9)');
      const insert = `INSERT INTO sink(k,v) ${select('docs', 'SELECT k')} RETURNING k,v`;
      await assert.rejects(async () => db.execute(insert), error => {
        assert.equal(error.code, 'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      assert.deepEqual(await db.all('SELECT k FROM sink ORDER BY k'), [[3n], [9n]]);
      await db.execute('DELETE FROM sink WHERE k=3');
      const retry = await db.execute(insert);
      assert.equal(retry.affected, 2n);
      assert.deepEqual(retry.rows, [[1n, 121n], [3n, 130n]]);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents, 3n);
      assert.equal(audit.indexEntries, 3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT k FROM sink'), [[3n]]);
    } finally { await db.close(); }
  }
});

test('correlated compound membership preserves sets and write recovery in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', 'INSERT INTO docs(k) VALUES(1),(2)',
        'CREATE TABLE native(k INTEGER)', 'INSERT INTO native VALUES(1),(2)',
        'CREATE TABLE b(k INTEGER)', 'INSERT INTO b VALUES(1),(3)',
        'CREATE TABLE nums(n INTEGER)', 'INSERT INTO nums VALUES(0),(1),(2),(NULL)',
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_k ON sink(k)',
        'INSERT INTO sink(k) VALUES(2)',
      ]) await db.execute(sql);
      const query = (source, operator) => 'SELECT k,(SELECT sum(CASE WHEN x.n IN(SELECT k ' + operator + ' SELECT CAST(k AS REAL)) THEN 1 WHEN x.n NOT IN(SELECT k ' + operator + ' SELECT CAST(k AS REAL)) THEN 10 ELSE 100 END) FROM nums x WHERE k IS k) AS v FROM ' + source + ' d LEFT JOIN b USING(k) ORDER BY k';
      for (const operator of ['UNION ALL', 'UNION', 'INTERSECT', 'EXCEPT']) {
        const expected = await db.execute(query('native', operator));
        const actual = await db.execute(query('docs', operator));
        assert.deepEqual(actual.columns, expected.columns);
        assert.deepEqual(actual.rows, expected.rows);
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(k) VALUES(9)');
      const insert = 'INSERT INTO sink(k,v) ' + query('docs', 'INTERSECT') + ' RETURNING k,v';
      await assert.rejects(async () => db.execute(insert), error => {
        assert.equal(error.code, 'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      assert.deepEqual(await db.all('SELECT k FROM sink ORDER BY k'), [[2n], [9n]]);
      await db.execute('DELETE FROM sink WHERE k=2');
      const retry = await db.execute(insert);
      assert.equal(retry.affected, 2n);
      assert.deepEqual(retry.rows, [[1n, 121n], [2n, 121n]]);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents, 3n);
      assert.equal(audit.indexEntries, 3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT k FROM sink'), [[2n]]);
    } finally { await db.close(); }
  }
});

test('ordered compound pagination binds parameters and preserves write recovery in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', 'INSERT INTO docs(k) VALUES(1),(2)',
        'CREATE TABLE native(k INTEGER)', 'INSERT INTO native VALUES(1),(2)',
        'CREATE TABLE b(k INTEGER)', 'INSERT INTO b VALUES(1),(3)',
        'CREATE TABLE nums(n INTEGER)', 'INSERT INTO nums VALUES(0),(1),(2),(NULL)',
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_k ON sink(k)',
        'INSERT INTO sink(k) VALUES(2)',
      ]) await db.execute(sql);
      const query = (source, operator, order) => {
        const projection = order === 'k DESC' ? 'k' : 'k AS chosen';
        const rhs = `SELECT ${projection} ${operator} SELECT NULL ORDER BY ${order} LIMIT $take OFFSET $skip`;
        return `SELECT k,(SELECT sum(CASE WHEN x.n IN(${rhs}) THEN 1 WHEN x.n NOT IN(${rhs}) THEN 10 ELSE 100 END) FROM nums x WHERE k IS k) AS v FROM ${source} d LEFT JOIN b USING(k) ORDER BY k`;
      };
      for (const operator of ['UNION ALL', 'UNION', 'INTERSECT', 'EXCEPT']) {
        for (const order of ['1', 'chosen DESC', 'k DESC']) {
          for (const params of [{ $take: 1n, $skip: 0n }, { $take: 1n, $skip: 1n }, { $take: 0n, $skip: 0n }]) {
            const expected = await db.execute(query('native', operator, order), params);
            const actual = await db.execute(query('docs', operator, order), params);
            assert.deepEqual(actual.columns, expected.columns);
            assert.deepEqual(actual.rows, expected.rows);
          }
        }
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(k) VALUES(9)');
      const insert = `INSERT INTO sink(k,v) ${query('docs', 'UNION', 'chosen DESC')} RETURNING k,v`;
      await assert.rejects(async () => db.execute(insert, { $take: 1n }), error => {
        assert.equal(error.code, 'FDB_PARAMETER');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      const params = { $take: 1n, $skip: 0n };
      const expectedRows = await db.all(query('native', 'UNION', 'chosen DESC'), params);
      await assert.rejects(async () => db.execute(insert, params), error => {
        assert.equal(error.code, 'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      assert.deepEqual(await db.all('SELECT k FROM sink ORDER BY k'), [[2n], [9n]]);
      await db.execute('DELETE FROM sink WHERE k=2');
      const retry = await db.execute(insert, params);
      assert.equal(retry.affected, 2n);
      assert.deepEqual(retry.rows, expectedRows);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents, 3n);
      assert.equal(audit.indexEntries, 3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT k FROM sink'), [[2n]]);
    } finally { await db.close(); }
  }
});

test('compound expression ordering preserves RIGHT JOIN client recovery', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', 'INSERT INTO docs(k) VALUES(1),(2)',
        'CREATE TABLE native(k INTEGER)', 'INSERT INTO native VALUES(1),(2)',
        'CREATE TABLE b(k INTEGER)', 'INSERT INTO b VALUES(1),(3)',
        'CREATE TABLE nums(n INTEGER)', 'INSERT INTO nums VALUES(0),(1),(2),(NULL)',
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_k ON sink(k)',
        'INSERT INTO sink(k) VALUES(3)',
      ]) await db.execute(sql);
      const query = (source, operator, order) => {
        const rhs = `SELECT k+1 ${operator} SELECT NULL ORDER BY ${order} LIMIT $take OFFSET $skip`;
        return `SELECT k,(SELECT sum(CASE WHEN x.n IN(${rhs}) THEN 1 WHEN x.n NOT IN(${rhs}) THEN 10 ELSE 100 END) FROM nums x WHERE k IS k) AS v FROM ${source} d RIGHT JOIN b USING(k) ORDER BY k`;
      };
      for (const operator of ['UNION ALL', 'UNION', 'INTERSECT', 'EXCEPT']) {
        for (const order of ['"k+1"', '"k+1" DESC']) {
          const params = { $take: 1n, $skip: 0n };
          const expected = await db.execute(query('native', operator, order), params);
          const actual = await db.execute(query('docs', operator, order), params);
          assert.deepEqual(actual.columns, expected.columns);
          assert.deepEqual(actual.rows, expected.rows);
        }
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(k) VALUES(9)');
      const insert = `INSERT INTO sink(k,v) ${query('docs', 'UNION', '"k+1" DESC')} RETURNING k,v`;
      await assert.rejects(async () => db.execute(insert, { $take: 1n }), error => {
        assert.equal(error.code, 'FDB_PARAMETER');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      const params = { $take: 1n, $skip: 0n };
      const expectedRows = await db.all(query('native', 'UNION', '"k+1" DESC'), params);
      await assert.rejects(async () => db.execute(insert, params), error => {
        assert.equal(error.code, 'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      assert.deepEqual(await db.all('SELECT k FROM sink ORDER BY k'), [[3n], [9n]]);
      await db.execute('DELETE FROM sink WHERE k=3');
      const retry = await db.execute(insert, params);
      assert.equal(retry.affected, 2n);
      assert.deepEqual(retry.rows, expectedRows);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents, 3n);
      assert.equal(audit.indexEntries, 3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT k FROM sink'), [[3n]]);
    } finally { await db.close(); }
  }
});

test('unordered UNION ALL pagination preserves parameters and client recovery', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', 'INSERT INTO docs(k) VALUES(1),(2)',
        'CREATE TABLE native(k INTEGER)', 'INSERT INTO native VALUES(1),(2)',
        'CREATE TABLE b(k INTEGER)', 'INSERT INTO b VALUES(1),(3)',
        'CREATE TABLE nums(n INTEGER)', 'INSERT INTO nums VALUES(0),(1),(2),(NULL)',
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_k ON sink(k)',
        'INSERT INTO sink(k) VALUES(2)',
      ]) await db.execute(sql);
      const query = source => {
        const rhs = 'SELECT NULL UNION ALL SELECT k LIMIT $take OFFSET $skip';
        return `SELECT k,(SELECT sum(CASE WHEN x.n IN(${rhs}) THEN 1 WHEN x.n NOT IN(${rhs}) THEN 10 ELSE 100 END) FROM nums x WHERE k IS k) AS v FROM ${source} d LEFT JOIN b USING(k) ORDER BY k`;
      };
      for (const params of [
        {$take:1n,$skip:0n}, {$take:1n,$skip:1n},
        {$take:0n,$skip:0n}, {$take:-1n,$skip:1n},
      ]) {
        const expected = await db.execute(query('native'),params);
        const actual = await db.execute(query('docs'),params);
        assert.deepEqual(actual.columns,expected.columns);
        assert.deepEqual(actual.rows,expected.rows);
        assert.deepEqual((await db.profileSelect(query('docs'),params)).result.rows,expected.rows);
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(k) VALUES(9)');
      const insert = `INSERT INTO sink(k,v) ${query('docs')} RETURNING k,v`;
      await assert.rejects(async () => db.execute(insert, { $take: 1n }), error => {
        assert.equal(error.code, 'FDB_PARAMETER');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      const params = { $take: 1n, $skip: 1n };
      const expectedRows = await db.all(query('native'), params);
      await assert.rejects(async () => db.execute(insert, params), error => {
        assert.equal(error.code, 'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      assert.deepEqual(await db.all('SELECT k FROM sink ORDER BY k'), [[2n], [9n]]);
      await db.execute('DELETE FROM sink WHERE k=2');
      const retry = await db.execute(insert, params);
      assert.equal(retry.affected, 2n);
      assert.deepEqual(retry.rows, expectedRows);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents, 3n);
      assert.equal(audit.indexEntries, 3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT k FROM sink'), [[2n]]);
    } finally { await db.close(); }
  }
});

test('unordered UNION pagination preserves parameters and client recovery', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', 'INSERT INTO docs(k) VALUES(1),(2)',
        'CREATE TABLE native(k INTEGER)', 'INSERT INTO native VALUES(1),(2)',
        'CREATE TABLE b(k INTEGER)', 'INSERT INTO b VALUES(1),(3)',
        'CREATE TABLE nums(n INTEGER)', 'INSERT INTO nums VALUES(0),(1),(2),(NULL)',
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_k ON sink(k)',
        'INSERT INTO sink(k) VALUES(2)',
      ]) await db.execute(sql);
      const query = source => {
        const rhs = 'SELECT NULL UNION SELECT k LIMIT $take OFFSET $skip';
        return `SELECT k,(SELECT sum(CASE WHEN x.n IN(${rhs}) THEN 1 WHEN x.n NOT IN(${rhs}) THEN 10 ELSE 100 END) FROM nums x WHERE k IS k) AS v FROM ${source} d LEFT JOIN b USING(k) ORDER BY k`;
      };
      for (const params of [
        {$take:1n,$skip:0n}, {$take:1n,$skip:1n},
        {$take:0n,$skip:0n}, {$take:-1n,$skip:1n},
      ]) {
        const expected = await db.execute(query('native'),params);
        const actual = await db.execute(query('docs'),params);
        assert.deepEqual(actual.columns,expected.columns);
        assert.deepEqual(actual.rows,expected.rows);
        assert.deepEqual((await db.profileSelect(query('docs'),params)).result.rows,expected.rows);
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(k) VALUES(9)');
      const insert = `INSERT INTO sink(k,v) ${query('docs')} RETURNING k,v`;
      await assert.rejects(async () => db.execute(insert, { $take: 1n }), error => {
        assert.equal(error.code, 'FDB_PARAMETER');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      const params = { $take: 1n, $skip: 1n };
      const expectedRows = await db.all(query('native'), params);
      await assert.rejects(async () => db.execute(insert, params), error => {
        assert.equal(error.code, 'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
        return true;
      });
      assert.deepEqual(await db.all('SELECT k FROM sink ORDER BY k'), [[2n], [9n]]);
      await db.execute('DELETE FROM sink WHERE k=2');
      const retry = await db.execute(insert, params);
      assert.equal(retry.affected, 2n);
      assert.deepEqual(retry.rows, expectedRows);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents, 3n);
      assert.equal(audit.indexEntries, 3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT k FROM sink'), [[2n]]);
    } finally { await db.close(); }
  }
});

test('typed compound parameters preserve projections and write recovery in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', 'CREATE TABLE probe(n INTEGER)', 'INSERT INTO probe VALUES(1)',
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_n ON sink(n)', 'INSERT INTO sink(n) VALUES(2)',
      ]) await db.execute(sql);
      const query = 'SELECT d.n,(SELECT $same FROM probe WHERE d.k IN(SELECT d.k INTERSECT SELECT $same LIMIT 1)) AS value FROM docs d ORDER BY d.n';
      for (const value of [true, 1n, new Record('docs',7n), Buffer.from('FDB\x01{"type":"Integer","value":7}')]) {
        await db.execute('INSERT INTO docs(n,k) VALUES(1,$same)', {$same:value});
        assert.deepEqual((await db.execute(query, {$same:value})).rows, [[1n,value]]);
        assert.deepEqual((await db.profileSelect(query, {$same:value})).result.rows, [[1n,value]]);
        await db.execute('DELETE FROM docs');
      }
      const value = new Record('docs',7n);
      await db.execute('INSERT INTO docs(n,k) VALUES(1,$same),(2,$same)', {$same:value});
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(n) VALUES(9)');
      const insert = 'INSERT INTO sink(n,value) ' + query + ' RETURNING n,value';
      await assert.rejects(async () => db.execute(insert, {$same:value}), error => {
        assert.equal(error.code,'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      assert.deepEqual(await db.all('SELECT n FROM sink ORDER BY n'),[[2n],[9n]]);
      await db.execute('DELETE FROM sink WHERE n=2');
      const retry = await db.execute(insert, {$same:value});
      assert.equal(retry.affected,2n);
      assert.deepEqual(retry.rows,[[1n,value],[2n,value]]);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents,3n);
      assert.equal(audit.indexEntries,3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT n FROM sink'),[[2n]]);
    } finally { await db.close(); }
  }
});

test('collated compound membership preserves client write recovery', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', 'CREATE TABLE baseline(k TEXT)',
        'CREATE TABLE probe(n INTEGER)', 'INSERT INTO probe VALUES(1)',
        "INSERT INTO docs(k) VALUES('A'),('a'),('a '),('B')",
        "INSERT INTO baseline VALUES('A'),('a'),('a '),('B')",
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_k ON sink(k)',
        "INSERT INTO sink(k) VALUES('B')",
      ]) await db.execute(sql);
      const query = (source, operator, collation) => `SELECT d.k,(SELECT count(*) FROM probe WHERE d.k COLLATE ${collation} IN(SELECT d.k COLLATE ${collation} ${operator} SELECT 'a' LIMIT $take OFFSET $skip)) AS found FROM ${source} d ORDER BY d.k`;
      for (const operator of ['UNION ALL','UNION','INTERSECT','EXCEPT']) {
        for (const collation of ['BINARY','NOCASE','RTRIM']) {
          for (const params of [{$take:1n,$skip:0n},{$take:1n,$skip:1n},{$take:0n,$skip:0n}]) {
            const expected = await db.execute(query('baseline',operator,collation),params);
            const actual = await db.execute(query('docs',operator,collation),params);
            assert.deepEqual(actual.columns,expected.columns);
            assert.deepEqual(actual.rows,expected.rows);
            assert.deepEqual((await db.profileSelect(query('docs',operator,collation),params)).result.rows,expected.rows);
          }
        }
      }
      await db.execute('BEGIN');
      await db.execute("INSERT INTO sink(k) VALUES('pending')");
      const select = query('docs','UNION ALL','NOCASE');
      const insert = 'INSERT INTO sink(k,found) ' + select + ' RETURNING k,found';
      const params = {$take:1n,$skip:0n};
      await assert.rejects(async () => db.execute(insert,{$take:1n}), error => {
        assert.equal(error.code,'FDB_PARAMETER');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      await assert.rejects(async () => db.execute(insert,params), error => {
        assert.equal(error.code,'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      assert.deepEqual(await db.all('SELECT k FROM sink ORDER BY k'),[['B'],['pending']]);
      await db.execute("DELETE FROM sink WHERE k='B'");
      const retry = await db.execute(insert,params);
      assert.equal(retry.affected,4n);
      assert.deepEqual(retry.rows,await db.all(query('baseline','UNION ALL','NOCASE'),params));
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents,5n);
      assert.equal(audit.indexEntries,5n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT k FROM sink'),[['B']]);
    } finally { await db.close(); }
  }
});

test('collated scalar ranges and between preserve client write recovery', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs', 'CREATE TABLE baseline(n INTEGER,a,b,c)',
        "INSERT INTO docs(n,a,b,c) VALUES(1,'beta','ALPHA','GAMMA'),(2,'a ','a','a'),(3,NULL,'a','z')",
        "INSERT INTO baseline VALUES(1,'beta','ALPHA','GAMMA'),(2,'a ','a','a'),(3,NULL,'a','z')",
        'CREATE TABLE sink', 'CREATE UNIQUE INDEX sink_n ON sink(n)',
        'INSERT INTO sink(n) VALUES(2)',
      ]) await db.execute(sql);
      const query = predicate => `SELECT d.n,(SELECT ${predicate} UNION ALL SELECT NULL LIMIT 1) AS value FROM docs d`;
      for (const collation of ['BINARY','NOCASE','RTRIM']) {
        for (const predicate of [
          `d.a < d.b COLLATE ${collation}`,
          `d.a COLLATE ${collation} >= d.b`,
          `d.a COLLATE ${collation} BETWEEN d.b AND d.c`,
          `d.a NOT BETWEEN d.b COLLATE ${collation} AND d.c COLLATE BINARY`,
        ]) {
          const native = await db.execute(`SELECT d.n,${predicate} AS value FROM baseline d`);
          const actual = await db.execute(query(predicate));
          assert.deepEqual(actual.columns,native.columns);
          assert.deepEqual(actual.rows,native.rows);
          assert.deepEqual((await db.profileSelect(query(predicate))).result.rows,native.rows);
        }
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(n) VALUES(9)');
      const predicate = 'd.a COLLATE NOCASE BETWEEN d.b AND d.c';
      const insert = 'INSERT INTO sink(n,value) ' + query(predicate) + ' RETURNING n,value';
      await assert.rejects(async () => db.execute(insert), error => {
        assert.equal(error.code,'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      assert.deepEqual(await db.all('SELECT n FROM sink ORDER BY n'),[[2n],[9n]]);
      await db.execute('DELETE FROM sink WHERE n=2');
      const retry = await db.execute(insert);
      assert.equal(retry.affected,3n);
      assert.deepEqual(retry.rows,await db.all(`SELECT d.n,${predicate} FROM baseline d`));
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents,4n);
      assert.equal(audit.indexEntries,4n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT n FROM sink'),[[2n]]);
    } finally { await db.close(); }
  }
});

test('local CTE merged keys preserve parameters and client write recovery', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs','CREATE TABLE baseline(n INTEGER)','CREATE TABLE keys(n INTEGER)',
        'INSERT INTO docs(n) VALUES(1),(2),(3)','INSERT INTO baseline VALUES(1),(2),(3)',
        'INSERT INTO keys VALUES(1),(4)',
        'CREATE TABLE sink','CREATE UNIQUE INDEX sink_n ON sink(n)','INSERT INTO sink(n) VALUES(4)',
      ]) await db.execute(sql);
      const query = (source,join,materialization,shadow) => `SELECT n,(WITH chosen AS ${materialization} (SELECT n+$shift AS m${shadow ? ',100 AS n' : ''} FROM baseline) SELECT max(m) FROM chosen WHERE m<n) AS prior FROM ${source} a ${join} keys b USING(n) ORDER BY n`;
      const params = {$shift:0n};
      for (const join of ['JOIN','LEFT JOIN','RIGHT JOIN']) {
        for (const materialization of ['','MATERIALIZED','NOT MATERIALIZED']) {
          for (const shadow of [false,true]) {
            const expected = await db.execute(query('baseline',join,materialization,shadow),params);
            const sql = query('docs',join,materialization,shadow);
            const actual = await db.execute(sql,params);
            assert.deepEqual(actual.columns,expected.columns);
            assert.deepEqual(actual.rows,expected.rows);
            assert.deepEqual((await db.profileSelect(sql,params)).result.rows,expected.rows);
          }
        }
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(n) VALUES(9)');
      const select = query('docs','RIGHT JOIN','MATERIALIZED',false);
      const insert = 'INSERT INTO sink(n,prior) ' + select + ' RETURNING n,prior';
      await assert.rejects(async () => db.execute(insert), error => {
        assert.equal(error.code,'FDB_PARAMETER');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      await assert.rejects(async () => db.execute(insert,params), error => {
        assert.equal(error.code,'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      assert.deepEqual(await db.all('SELECT n FROM sink ORDER BY n'),[[4n],[9n]]);
      await db.execute('DELETE FROM sink WHERE n=4');
      const retry = await db.execute(insert,params);
      assert.equal(retry.affected,2n);
      assert.deepEqual(retry.rows,[[1n,null],[4n,3n]]);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents,3n);
      assert.equal(audit.indexEntries,3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT n FROM sink'),[[4n]]);
    } finally { await db.close(); }
  }
});

test('ordered local CTE pagination preserves typed client recovery', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const open of [() => new Database(), () => AsyncDatabase.open()]) {
    const db = await open();
    try {
      for (const sql of [
        'CREATE TABLE docs','CREATE TABLE baseline(n INTEGER)','CREATE TABLE keys(n INTEGER)',
        'INSERT INTO docs(n) VALUES(1),(2),(3)','INSERT INTO baseline VALUES(1),(2),(3)',
        'INSERT INTO keys VALUES(1),(4)',
        'CREATE TABLE sink','CREATE UNIQUE INDEX sink_n ON sink(n)','INSERT INTO sink(n) VALUES(4)',
      ]) await db.execute(sql);
      const query = (materialization,direction) => `SELECT n,(WITH chosen AS ${materialization} (SELECT n AS m FROM baseline) SELECT $value FROM chosen WHERE m<n ORDER BY m ${direction} LIMIT $take OFFSET $skip) AS value FROM docs a RIGHT JOIN keys b USING(n) ORDER BY n`;
      for (const materialization of ['','MATERIALIZED','NOT MATERIALIZED']) {
        for (const direction of ['ASC','DESC']) {
          for (const [take,skip] of [[0n,0n],[1n,0n],[1n,1n],[1n,3n],[-1n,1n]]) {
            const params = {$take:take,$skip:skip,$value:true};
            const expected = [[1n,null],[4n,take === 0n || skip === 3n ? null : true]];
            const sql = query(materialization,direction);
            assert.deepEqual((await db.execute(sql,params)).rows,expected);
            assert.deepEqual((await db.profileSelect(sql,params)).result.rows,expected);
          }
        }
      }
      for (const [take,skip,kept] of [
        ['1','0',true],['1.0','1e0',true],[' +1 ',true,true],
        [true,false,true],[1,1,true],['.','0',false],['-1','3',false],
      ]) {
        const params = {$take:take,$skip:skip,$value:true};
        const expected = [[1n,null],[4n,kept ? true : null]];
        const sql = query('MATERIALIZED','DESC');
        assert.deepEqual((await db.execute(sql,params)).rows,expected);
        assert.deepEqual((await db.profileSelect(sql,params)).result.rows,expected);
      }
      await db.execute('BEGIN');
      await db.execute('INSERT INTO sink(n) VALUES(9)');
      const insert = 'INSERT INTO sink(n,value) ' + query('MATERIALIZED','DESC') + ' RETURNING n,value';
      const value = new Record('docs',7n);
      const params = {$take:'1.0',$skip:'1e0',$value:value};
      await assert.rejects(async () => db.execute(insert,{$take:1n,$value:value}), error => {
        assert.equal(error.code,'FDB_PARAMETER');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      await assert.rejects(async () => db.execute(insert,{$take:'1',$value:value}), error => {
        assert.equal(error.code,'FDB_PARAMETER');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      await assert.rejects(async () => db.execute(insert,params), error => {
        assert.equal(error.code,'FDB_CONSTRAINT');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      assert.deepEqual(await db.all('SELECT n FROM sink ORDER BY n'),[[4n],[9n]]);
      await db.execute('DELETE FROM sink WHERE n=4');
      const retry = await db.execute(insert,params);
      assert.equal(retry.affected,2n);
      assert.deepEqual(retry.rows,[[1n,null],[4n,value]]);
      const audit = await db.checkCollectionIntegrity('sink');
      assert.equal(audit.documents,3n);
      assert.equal(audit.indexEntries,3n);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT n FROM sink'),[[4n]]);
    } finally { await db.close(); }
  }
});

test('bounded SELECT limits preserve typed results and transactions in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const db of [new Database(), await AsyncDatabase.open()]) {
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('CREATE TABLE native(n INTEGER)');
      await db.execute('INSERT INTO native VALUES(1),(2)');
      const payload = { text: '猫', ref: new Record('docs', 7n), bytes: Buffer.from([0,255]), array: [true, null] };
      await db.execute('INSERT INTO docs {value:$value}', {$value: payload});
      const sql = 'SELECT value AS v FROM docs';
      // v=1; object keys=17; text=3; record=12; binary=2; array=2.
      const exact = {maxRows:1n, maxPayloadBytes:37n};
      const expected = await db.execute(sql);
      assert.deepEqual((await db.selectWithLimits(sql, exact)).rows, expected.rows);
      assert.deepEqual((await db.profileSelectWithLimits(sql, exact)).result.rows, expected.rows);
      await db.execute('BEGIN');
      await db.execute('INSERT INTO native VALUES(3)');
      for (const [query, limits] of [[sql, {...exact, maxPayloadBytes:36n}], [sql, {...exact,maxRows:0n}], ['SELECT n FROM native', {maxRows:2n,maxPayloadBytes:100n}], ['SELECT n FROM native WHERE 0',{maxRows:0n,maxPayloadBytes:0n}]]) {
        await assert.rejects(Promise.resolve().then(() => db.selectWithLimits(query, limits)), e => {
          assert.equal(e.code, 'FDB_LIMIT');
          assert.deepEqual(e.transaction, {before:'active',after:'active'});
          return true;
        });
      }
      assert.deepEqual((await db.selectWithLimits('SELECT n FROM native WHERE 0',{maxRows:0n,maxPayloadBytes:1n})).rows, []);
      await assert.rejects(Promise.resolve().then(() => db.selectWithLimits('DELETE FROM native', exact)), e => e.code === 'FDB_UNSUPPORTED');
      assert.equal((await db.selectWithLimits('SELECT n FROM native',{maxRows:3n,maxPayloadBytes:25n})).rows.length, 3);
      for (const limits of [undefined, null, {}, {maxRows:1,maxPayloadBytes:1n}, {maxRows:-1n,maxPayloadBytes:1n}, {maxRows:1n,maxPayloadBytes:2n**64n}, {...exact,typo:1n}]) {
        await assert.rejects(Promise.resolve().then(() => db.selectWithLimits(sql,limits)), e => e instanceof TypeError || e instanceof RangeError);
      }
      await db.execute('ROLLBACK');
      assert.equal((await db.all('SELECT n FROM native')).length, 2);
    } finally { await db.close(); }
    await assert.rejects(Promise.resolve().then(() => db.selectWithLimits('SELECT 1',{maxRows:1n,maxPayloadBytes:100n})), /clos/i);
  }
});

test('bounded worker SELECT cancellation cleans up and allows retry', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const db = await AsyncDatabase.open();
  const limits = {maxRows:1n,maxPayloadBytes:100n};
  try {
    const cancelled = new AbortController(); cancelled.abort();
    await assert.rejects(db.selectWithLimits('SELECT 1',limits,{}, {signal:cancelled.signal}), e => e.code === 'FDB_CANCELLED');
    await db.execute('CREATE TABLE nums(n INTEGER)');
    await db.execute('INSERT INTO nums VALUES ' + Array.from({length:1000}, (_,i) => '(' + (i+1) + ')').join(','));
    await db.execute('BEGIN');
    await db.execute('INSERT INTO nums VALUES(1001)');
    const active = new AbortController();
    const pending = db.profileSelectWithLimits('SELECT count(*) FROM nums a CROSS JOIN nums b CROSS JOIN nums c',limits,{}, {signal:active.signal});
    const timer = setTimeout(() => active.abort(), 25);
    try { await assert.rejects(pending, e => { assert.equal(e.code, 'FDB_CANCELLED'); assert.deepEqual(e.transaction, {before:'active',after:'active'}); return true; }); }
    finally { clearTimeout(timer); }
    assert.deepEqual((await db.selectWithLimits('SELECT count(*) AS n FROM nums',limits)).rows, [[1001n]]);
    await db.execute('ROLLBACK');
    assert.deepEqual((await db.selectWithLimits('SELECT count(*) AS n FROM nums',limits)).rows, [[1000n]]);
  } finally { await db.close(); }
});

test('bounded FETCH shares final payload limits across duplicates and scalar columns', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const db of [new Database(), await AsyncDatabase.open()]) {
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('INSERT INTO docs {id:docs:a,n:7}');
      await db.execute('CREATE TABLE positions(n INTEGER)');
      await db.execute('INSERT INTO positions VALUES(1),(2)');
      await db.execute('BEGIN');
      await db.execute('INSERT INTO docs {id:docs:pending,n:9}');
      for (const [sql, bytes, rows] of [
        ['SELECT record::fetch(docs:a) AS v',17n,1n],
        ['SELECT record::fetch(docs:a) AS v FROM positions',33n,2n],
        ['SELECT record::fetch(docs:a) AS v,record::fetch(docs:a) AS w,1 AS n',43n,1n],
        ['SELECT record::fetch(docs:verylongmissingkey) AS v FROM positions',3n,2n],
        ['SELECT record::fetch(NULL) AS v',2n,1n],
      ]) {
        const limits = {maxRows:rows,maxPayloadBytes:bytes};
        const expected = await db.profileSelect(sql);
        const actual = await db.profileSelectWithLimits(sql,limits);
        assert.deepEqual(actual.result,expected.result);
        assert.equal(actual.metrics.fetchBatches,expected.metrics.fetchBatches);
        for (const rejected of [{...limits,maxPayloadBytes:bytes-1n},{...limits,maxRows:rows-1n}]) {
          await assert.rejects(Promise.resolve().then(() => db.selectWithLimits(sql,rejected)), e => {
            assert.equal(e.code,'FDB_LIMIT');
            assert.deepEqual(e.transaction,{before:'active',after:'active'});
            return true;
          });
        }
        assert.deepEqual((await db.selectWithLimits(sql,limits)).rows,expected.result.rows);
      }
      await db.execute('ROLLBACK');
      assert.equal((await db.all('SELECT n FROM docs')).length,1);
    } finally { await db.close(); }
  }
});

test('atomic write result policy restores rows indexes and trigger effects in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  for (const db of [new Database(), await AsyncDatabase.open()]) {
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
      await db.execute('CREATE TABLE native(n INTEGER UNIQUE)');
      await db.execute('CREATE TABLE audit(n INTEGER)');
      await db.execute('CREATE TRIGGER native_audit AFTER INSERT ON native BEGIN INSERT INTO audit VALUES(new.n); END');
      await db.execute('BEGIN');
      await db.execute('INSERT INTO docs {id:docs:prior,n:9}');
      await db.execute('INSERT INTO native VALUES(9)');
      for (const table of ['docs','native']) {
        const sql = 'INSERT INTO ' + table + '(n) VALUES(1),(2) RETURNING n';
        for (const limits of [{maxRows:1n,maxPayloadBytes:17n},{maxRows:2n,maxPayloadBytes:16n}]) {
          await assert.rejects(Promise.resolve().then(() => db.writeWithResultLimits(sql,limits)), e => {
            assert.equal(e.code,'FDB_LIMIT');
            assert.deepEqual(e.transaction,{before:'active',after:'active'});
            return true;
          });
          assert.deepEqual(await db.all('SELECT n FROM '+table),[[9n]]);
        }
        const result = await db.writeWithResultLimits(sql,{maxRows:2n,maxPayloadBytes:17n});
        assert.deepEqual(result.rows,[[1n],[2n]]);
        assert.equal(result.affected,2n);
        assert.deepEqual(result.transaction,{before:'active',after:'active'});
      }
      assert.deepEqual(await db.all('SELECT n FROM audit ORDER BY n'),[[1n],[2n],[9n]]);
      const mixed = 'INSERT INTO native(n) SELECT n+$offset FROM docs ORDER BY n RETURNING n';
      const mixedBudget = {maxRows:3n,maxPayloadBytes:25n};
      for (const rejected of [{...mixedBudget,maxPayloadBytes:0n},{...mixedBudget,maxPayloadBytes:24n},{...mixedBudget,maxRows:2n}]) {
        await assert.rejects(Promise.resolve().then(() => db.writeWithResultLimits(mixed,rejected,{$offset:20n})), error => {
          assert.equal(error.code,'FDB_LIMIT');
          assert.deepEqual(error.transaction,{before:'active',after:'active'});
          return true;
        });
        assert.deepEqual(await db.all('SELECT n FROM native ORDER BY n'),[[1n],[2n],[9n]]);
        assert.deepEqual(await db.all('SELECT n FROM audit ORDER BY n'),[[1n],[2n],[9n]]);
      }
      assert.deepEqual((await db.writeWithResultLimits(mixed,mixedBudget,{$offset:20n})).rows,[[21n],[22n],[29n]]);

      const value = {r:new Record('docs','prior'),b:Buffer.from([0,255]),t:'猫',a:[true,null]};
      const sql = 'UPDATE docs:prior {value:$v} RETURNING value AS v';
      const exact = {maxRows:1n,maxPayloadBytes:21n}; // v + keys 4 + record 9 + binary 2 + text 3 + array 2
      await assert.rejects(Promise.resolve().then(() => db.writeWithResultLimits(sql,{...exact,maxPayloadBytes:20n},{$v:value})), e => e.code === 'FDB_LIMIT');
      assert.deepEqual(await db.exactlyOne('SELECT value FROM docs WHERE n=9'),[null]);
      assert.deepEqual((await db.writeWithResultLimits(sql,exact,{$v:value})).rows,[[value]]);
      assert.equal((await db.checkCollectionIntegrity('docs')).documents,3n);
      for (const sql of ['COMMIT','ROLLBACK','CREATE TABLE nope(n)','SELECT 1']) {
        await assert.rejects(Promise.resolve().then(() => db.writeWithResultLimits(sql,exact)), e => e.code === 'FDB_UNSUPPORTED');
      }
      await assert.rejects(Promise.resolve().then(() => db.writeWithResultLimits('INSERT INTO native VALUES(5)',{maxRows:0,maxPayloadBytes:0n})), TypeError);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT n FROM native'),[]);
      assert.deepEqual(await db.all('SELECT n FROM audit'),[]);
      assert.equal((await db.checkCollectionIntegrity('docs')).documents,0n);
    } finally { await db.close(); }
    await assert.rejects(Promise.resolve().then(() => db.writeWithResultLimits('DELETE FROM docs',{maxRows:0n,maxPayloadBytes:0n})), /clos/i);
  }
});

test('worker write result cancellation rolls back and permits queued retry', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const db = await AsyncDatabase.open();
  const limits = {maxRows:1n,maxPayloadBytes:9n};
  try {
    await db.execute('CREATE TABLE nums(n INTEGER)');
    await db.execute('INSERT INTO nums VALUES '+Array.from({length:300},(_,n)=>'('+n+')').join(','));
    await db.execute('CREATE TABLE docs');
    await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
    await db.execute('BEGIN');
    await db.execute('INSERT INTO docs {id:docs:prior,n:9}');
    const before = new AbortController(); before.abort();
    await assert.rejects(db.writeWithResultLimits('DELETE FROM docs',limits,{}, {signal:before.signal}), e => e.code === 'FDB_CANCELLED');
    const active = new AbortController();
    const pending = db.writeWithResultLimits('INSERT INTO docs(n) SELECT count(*) FROM nums a CROSS JOIN nums b CROSS JOIN nums c RETURNING n',limits,{}, {signal:active.signal});
    const following = db.all('SELECT n FROM docs');
    const settled = Promise.allSettled([pending,following]);
    const timer = setTimeout(()=>active.abort(),20);
    const [cancelled,recovered] = await settled;
    clearTimeout(timer);
    assert.equal(cancelled.status,'rejected');
    assert.equal(cancelled.reason.code,'FDB_CANCELLED');
    assert.deepEqual(cancelled.reason.transaction,{before:'active',after:'active'});
    assert.equal(recovered.status,'fulfilled');
    assert.deepEqual(recovered.value,[[9n]]);
    assert.equal(require('node:events').getEventListeners(active.signal,'abort').length,0);
    assert.deepEqual((await db.writeWithResultLimits('INSERT INTO docs {n:1} RETURNING n',limits)).rows,[[1n]]);
    assert.equal((await db.checkCollectionIntegrity('docs')).documents,2n);
    await db.execute('ROLLBACK');
    assert.deepEqual(await db.all('SELECT n FROM docs'),[]);
  } finally { await db.close(); }
});

test('connection write buffer limits preserve pending work in both clients', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const options = { writeBufferLimits: { maxRows: 1n, maxPayloadBytes: 1000n } };
  for (const db of [new Database(':memory:', options), await AsyncDatabase.open(':memory:', options)]) {
    try {
      await db.execute('CREATE TABLE docs');
      await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
      await db.execute('INSERT INTO docs {id:docs:a,n:1}');
      await db.execute('BEGIN');
      await db.execute('INSERT INTO docs {id:docs:b,n:2}');
      for (const sql of ['UPDATE docs SET n=n+10', 'UPDATE docs {n:n+10}', 'DELETE FROM docs', 'INSERT INTO docs(n) VALUES(3),(4)']) {
        await assert.rejects(async () => db.execute(sql), error => {
          assert.equal(error.code, 'FDB_LIMIT');
          assert.deepEqual(error.transaction, { before: 'active', after: 'active' });
          return true;
        });
        assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'), [[1n], [2n]]);
      }
      await db.execute('UPDATE docs SET n=3 WHERE n=2');
      assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'), [[1n], [3n]]);
      await db.execute('ROLLBACK');
      assert.deepEqual(await db.all('SELECT n FROM docs'), [[1n]]);
      assert.equal((await db.checkCollectionIntegrity('docs')).documents, 1n);
    } finally { await db.close(); }
  }
});

test('connection options reject invalid limits before opening a database', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'fastdb-options-'));
  const file = path.join(dir, 'absent.db');
  try {
    for (const options of [null, [], { unknown: true }, { writeBufferLimits: {} },
      { writeBufferLimits: { maxRows: 1, maxPayloadBytes: 2n } },
      { writeBufferLimits: { maxRows: -1n, maxPayloadBytes: 2n } },
      { writeBufferLimits: { maxRows: 1n, maxPayloadBytes: 2n ** 64n } }]) {
      assert.throws(() => new Database(file, options), /options|option|limits/);
      await assert.rejects(AsyncDatabase.open(file, options), /options|option|limits/);
      assert.equal(fs.existsSync(file), false);
    }
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('worker timeout uses native deadline and preserves queued request isolation', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const db = await AsyncDatabase.open();
  try {
    await db.execute('CREATE TABLE input(n)');
    await db.execute('INSERT INTO input VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9)');
    await db.execute('CREATE TABLE docs');
    await db.execute('BEGIN');
    await db.execute('INSERT INTO docs {n:1}');
    const sql = 'SELECT count(*) FROM input a,input b,input c,input d,input e,input f,input g,input h,input i,input j';
    const active = db.execute(sql, {}, { timeoutMs: 20 });
    const queued = db.execute('INSERT INTO docs {n:2}', {}, { timeoutMs: 0 });
    const next = db.all('SELECT n FROM docs');
    await Promise.all([active, queued].map(promise => assert.rejects(promise, error => {
      assert.equal(error.code, 'FDB_CANCELLED');
      assert.deepEqual(error.transaction, { before:'active', after:'active' });
      return true;
    })));
    assert.deepEqual(await next, [[1n]]);
    for (const timeoutMs of [-1, 0.5, NaN, Infinity, 4294967296, '1', 1n]) {
      await assert.rejects(db.execute('INSERT INTO docs {n:9}', {}, { timeoutMs }), /timeoutMs/);
    }
    const controller = new AbortController();
    controller.abort();
    await assert.rejects(db.execute('INSERT INTO docs {n:9}', {}, { signal:controller.signal, timeoutMs:60000 }), {code:'FDB_CANCELLED'});
    await db.execute('INSERT INTO docs {n:3}', {}, { timeoutMs:60000 });
    assert.deepEqual(await db.all('SELECT n FROM docs ORDER BY n'), [[1n],[3n]]);
    await db.execute('ROLLBACK');
    assert.deepEqual(await db.all('SELECT * FROM docs'), []);
  } finally { await db.close(); }
});

test('zero timeout rejects async operations before work and allows recovery', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const db = await AsyncDatabase.open();
  const limits = {maxRows:10n,maxPayloadBytes:1000n};
  const expired = {timeoutMs:0};
  const plan = [{version:1n,name:'initial',sql:'CREATE TABLE docs;'}];
  try {
    await db.migrate(plan);
    await db.execute('INSERT INTO docs {id:docs:a,n:1}');
    const payload = await db.exportDocuments('docs');
    for (const operation of [
      () => db.execute('DELETE FROM docs',{},expired),
      () => db.profileSelect('SELECT * FROM docs',{},expired),
      () => db.selectWithLimits('SELECT * FROM docs',limits,{},expired),
      () => db.profileSelectWithLimits('SELECT * FROM docs',limits,{},expired),
      () => db.writeWithResultLimits('DELETE FROM docs RETURNING n',limits,{},expired),
      () => db.checkCollectionIntegrity('docs',{},expired),
      () => db.executeBatch('DELETE FROM docs; SELECT 1;',expired),
      () => db.exportDocuments('docs','json',expired),
      () => db.importDocuments('docs',payload,'json',expired),
      () => db.migrate([...plan,{version:2n,name:'delete',sql:'DELETE FROM docs;'}],expired),
    ]) {
      await assert.rejects(operation(), error => {
        assert.equal(error.code,'FDB_CANCELLED');
        assert.deepEqual(error.transaction,{before:'autocommit',after:'autocommit'});
        return true;
      });
      assert.deepEqual(await db.all('SELECT n FROM docs'),[[1n]]);
    }
    assert.equal((await db.migrate(plan)).alreadyApplied,1);
    assert.equal((await db.checkCollectionIntegrity('docs')).documents,1n);
    await db.execute('UPDATE docs SET n=2',{}, {timeoutMs:60000});
    assert.deepEqual(await db.all('SELECT n FROM docs'),[[2n]]);
  } finally { await db.close(); }
});

test('timeout completion and invalid signal paths release native token capacity', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const { createCancellationToken, releaseCancellationToken } = require('./native.cjs');
  const db = await AsyncDatabase.open();
  const held = [];
  try {
    for (let i=0; i<256; i++) {
      await assert.rejects(db.execute('SELECT 1',{}, {timeoutMs:0}), {code:'FDB_CANCELLED'});
      await assert.rejects(db.execute('SELECT 1',{}, {timeoutMs:60000,signal:null}), TypeError);
      assert.deepEqual(await db.all('SELECT 1',{}, {timeoutMs:60000}), [[1n]]);
    }
    // The private native registry has a fixed 16,384-token cap. Filling all
    // slots proves these completed requests retained no registry entries.
    for (let i=0; i<16384; i++) held.push(createCancellationToken());
    assert.throws(() => createCancellationToken(), /token limit/);
    for (const key of held.splice(0)) releaseCancellationToken(key);
    assert.deepEqual(await db.all('SELECT 2',{}, {timeoutMs:60000}), [[2n]]);
  } finally {
    for (const key of held) releaseCancellationToken(key);
    await db.close();
  }
});

test('closing a worker with a timed write permits clean persistent reopen', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const dir = fs.mkdtempSync(path.join(os.tmpdir(),'fastdb-timeout-close-'));
  const file = path.join(dir,'database.db');
  let db;
  try {
    db = await AsyncDatabase.open(file);
    await db.execute('CREATE TABLE input(n)');
    await db.execute('INSERT INTO input VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9)');
    await db.execute('CREATE TABLE docs');
    await db.execute('CREATE UNIQUE INDEX docs_n ON docs(n)');
    await db.execute('INSERT INTO docs {id:docs:a,n:1}');
    await db.execute('BEGIN');
    await db.execute('INSERT INTO docs {id:docs:b,n:2}');
    const operation = db.execute('INSERT INTO docs(n) SELECT count(*) FROM input a,input b,input c,input d,input e,input f,input g,input h,input i,input j', {}, {timeoutMs:20});
    const rejection = assert.rejects(operation, error => {
      assert.equal(error.code,'FDB_CANCELLED');
      assert.deepEqual(error.transaction,{before:'active',after:'active'});
      return true;
    });
    const closed = db.close();
    assert.equal(db.close(),closed);
    await assert.rejects(db.execute('SELECT 1'),{code:'FDB_CLOSED'});
    await Promise.all([rejection,closed]);
    db = undefined;
    const reopened = new Database(file);
    try {
      assert.deepEqual(reopened.all('SELECT n FROM docs'),[[1n]]);
      assert.equal(reopened.checkCollectionIntegrity('docs').documents,1n);
      reopened.execute('INSERT INTO docs {id:docs:b,n:2}');
      assert.deepEqual(reopened.all('SELECT n FROM docs ORDER BY n'),[[1n],[2n]]);
    } finally { reopened.close(); }
  } finally {
    if (db) await db.close();
    fs.rmSync(dir,{recursive:true,force:true});
  }
});

test('direct JSON response composition preserves escaped names and nested values', async () => {
  const { AsyncDatabase } = require('./index.cjs');
  const value = { '"}]\nไทย': ['\0\t\\"', new Record('docs','"}]\n'), Buffer.from([0,34,92,255]), -0] };
  for (const db of [new Database(), await AsyncDatabase.open()]) {
    try {
      const sql = 'SELECT $value AS "quoted""name"';
      const result = await db.execute(sql, {$value:value});
      assert.deepEqual(result.columns,['quoted"name']);
      assert.deepEqual(result.rows,[[value]]);
      assert.deepEqual((await db.profileSelect(sql, {$value:value})).result.rows,[[value]]);
      const batch = await db.executeBatch("SELECT '}]\\\"' AS text; SELECT 2 AS n;");
      assert.equal(batch.length,2);
      assert.deepEqual(batch[1].result.rows,[[2n]]);
    } finally { await db.close(); }
  }
});

test('maximum logical depth survives wire framing, storage and transfer', async () => {
  const {AsyncDatabase} = require('./index.cjs');
  for (const db of [new Database(), await AsyncDatabase.open()]) {
    try {
      let value = new Record('docs','leaf');
      for(let i=0;i<64;i++) value = i%2 ? {nested:value} : [value];
      assert.deepEqual((await db.execute('SELECT $v AS v',{$v:value})).rows,[[value]]);
      await assert.rejects(async()=>db.execute('SELECT $v',{$v:[value]}),/nesting exceeds 64/);
      const payload = value.nested;
      await db.execute('CREATE TABLE docs');
      await db.execute('INSERT INTO docs {id:docs:deep,payload:$v}',{$v:payload});
      assert.deepEqual((await db.execute('SELECT payload FROM docs')).rows,[[payload]]);
      for(const format of ['json','ndjson']) {
        const data = await db.exportDocuments('docs',format);
        await db.execute('BEGIN');
        await db.execute('DELETE FROM docs');
        await db.importDocuments('docs',data,format);
        assert.deepEqual((await db.execute('SELECT payload FROM docs')).rows,[[payload]]);
        await db.execute('ROLLBACK');
      }
    } finally { await db.close(); }
  }
});

test('migration failures expose lossless versions, UTF-8 offsets and typed causes', async () => {
  const {AsyncDatabase,isFastDBError} = require('./index.cjs');
  for(const worker of [false,true]) for(const limited of [false,true]) {
    const options = limited ? {writeBufferLimits:{maxRows:1n,maxPayloadBytes:1000n}} : {};
    const db = worker ? await AsyncDatabase.open(':memory:',options) : new Database(':memory:',options);
    try {
      const base = {version:1n,name:'base',sql:'CREATE TABLE docs; CREATE UNIQUE INDEX docs_n ON docs(n); INSERT INTO docs {id:docs:saved,n:1};'};
      await db.migrate([base]);
      const prefix = "-- café 日本語\nINSERT INTO docs {id:docs:temp,n:2}; ";
      const bad = limited ? 'INSERT INTO docs (n) VALUES(3),(4);' : 'INSERT INTO docs {n:1};';
      const pending = {version:9007199254740993n,name:'pending',sql:prefix+bad};
      await assert.rejects(async()=>db.migrate([base,pending]),error=>{
        assert(isFastDBError(error));
        assert.equal(error.code,'FDB_MIGRATION');
        assert.equal(error.migration.version,pending.version);
        assert.equal(error.migration.offset,BigInt(Buffer.byteLength(prefix)));
        assert.equal(error.migration.cause.code,limited?'FDB_LIMIT':'FDB_CONSTRAINT');
        assert.equal(typeof error.migration.cause.message,'string');
        assert.deepEqual(error.transaction,{before:'autocommit',after:'autocommit'});
        return true;
      });
      assert.deepEqual((await db.execute('SELECT n FROM docs')).rows,[[1n]]);
      assert.equal((await db.checkCollectionIntegrity('docs')).documents,1n);
      pending.sql=prefix;
      assert.deepEqual((await db.migrate([base,pending])).applied,[pending.version]);
      assert.equal((await db.migrate([base,pending])).alreadyApplied,2);
    } finally { await db.close(); }
  }
  for(const migration of [null,{}, {version:1,offset:0n,cause:{code:'FDB_LIMIT',message:'x'}},
    {version:1n,offset:-1n,cause:{code:'FDB_LIMIT',message:'x'}},
    {version:1n,offset:0n,cause:{code:'bad',message:'x'}}]) {
    assert.equal(isFastDBError(Object.assign(new Error('test'),{code:'FDB_MIGRATION',migration})),false);
  }
});
