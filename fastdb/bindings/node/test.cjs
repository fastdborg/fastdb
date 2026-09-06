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
