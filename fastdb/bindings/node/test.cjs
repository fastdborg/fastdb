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
    zero: -0, fraction: 1.25, yes: true, nil: null, text: 'docs:p1', bytes: Buffer.from([0,255]),
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
  assert.deepEqual(db.exactlyOne('SELECT array::new($value) AS value', { $value: value })[0][0], value);
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
