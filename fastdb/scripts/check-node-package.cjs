'use strict';
// Maintainer smoke: build with check-node.sh first. No publishing or registry
// access; all artifacts and the consumer installation live in a temp directory.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const packageDir = path.resolve(__dirname, '../bindings/node');
const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
assert(fs.existsSync(path.join(packageDir, 'fastdb.node')), 'Build the addon with fastdb/scripts/check-node.sh first');
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'fastdb-package-'));
const run = (command, args, cwd) => execFileSync(command, args, {
  cwd, encoding: 'utf8', timeout: 120000, maxBuffer: 2 * 1024 * 1024,
  env: { ...process.env, NODE_PATH: '' },
});
try {
  const [packed] = JSON.parse(run(npm, ['pack', '--offline', '--ignore-scripts', '--json', '--pack-destination', temporary], packageDir));
  assert.deepEqual(packed.files.map(file => file.path).sort(), [
    'LICENSE.md', 'README.md', 'fastdb.node', 'index.cjs', 'index.d.ts', 'package.json', 'worker.cjs', 'native.cjs',
  ].sort());
  assert(packed.files.find(file => file.path === 'fastdb.node').size > 0);
  const consumer = path.join(temporary, 'consumer');
  fs.mkdirSync(consumer);
  fs.writeFileSync(path.join(consumer, 'package.json'), JSON.stringify({ name: 'fastdb-package-smoke', version: '0.0.0', private: true }));
  run(npm, ['install', '--offline', '--ignore-scripts', '--no-audit', '--no-fund', '--package-lock=false', path.join(temporary, packed.filename)], consumer);
  fs.writeFileSync(path.join(consumer, 'smoke.cjs'), `
'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const { Database, AsyncDatabase, Record, Vector } = require('@fastdb/node');
assert(require.resolve('@fastdb/node').startsWith(path.join(__dirname, 'node_modules')));
(async () => {
  const file = path.join(__dirname, 'database.db');
  const db = new Database(file);
  try {
    db.execute('CREATE TABLE docs');
    db.execute('CREATE UNIQUE INDEX docs_value ON docs(value)');
    db.execute('INSERT INTO docs (id,value) VALUES ($id,$value)', { $id: new Record('docs','saved'), $value: 9223372036854775807n });
    const audit = db.checkCollectionIntegrity('docs', {maxDocuments: 1n});
    assert.equal(audit.documents, 1n);
    assert.equal(audit.indexEntries, 1n);
    assert.ok(audit.encodedBytes > 0n);
    assert.equal(db.checkCollectionIntegrity('docs', {maxEncodedBytes: audit.encodedBytes}).encodedBytes, audit.encodedBytes);
    const profile = db.profileSelect('SELECT id,value FROM docs WHERE value=$value', {$value: 9223372036854775807n});
    assert(profile.result.rows[0][0] instanceof Record);
    assert.equal(profile.result.rows[0][1], 9223372036854775807n);
    assert.ok(profile.metrics.vmSteps > 0n);
    assert.ok(profile.metrics.btreeSeeks > 0n);
    assert.equal(typeof profile.metrics.indexSteps, 'bigint');
    assert.equal(profile.metrics.rowsWritten, 0n);
    for (const make of [Vector.float32, Vector.float64, Vector.sparse32, Vector.quantized8, Vector.bit1, () => Vector.sparse32Entries(3, [[0,1],[2,-1]])]) {
      const vector = make([1,0,-1]);
      assert.deepEqual(db.exactlyOne('SELECT $v AS v', {$v: vector})[0], vector);
    }
  } finally { db.close(); }
  const worker = await AsyncDatabase.open(file);
  try {
    for (const make of [Vector.float32, Vector.float64, Vector.sparse32, Vector.quantized8, Vector.bit1, () => Vector.sparse32Entries(3, [[0,1],[2,-1]])]) {
      const vector = make(new Float32Array([1,0,-1]));
      assert.deepEqual((await worker.exactlyOne('SELECT $v AS v', {$v: vector}))[0], vector);
    }
    const row = await worker.exactlyOne('SELECT id,value FROM docs');
    assert(row[0] instanceof Record);
    assert.equal(row[0].key, 'saved');
    assert.equal(row[1], 9223372036854775807n);
    await worker.execute('BEGIN');
    await worker.execute('UPDATE docs SET value=7');
    await assert.rejects(worker.checkCollectionIntegrity('docs', {maxDocuments: 0n}), error => error.code === 'FDB_LIMIT' && error.transaction.after === 'active');
    const profile = await worker.profileSelect('SELECT value FROM docs WHERE value=$value', {$value: 7n});
    assert.equal(profile.result.rows[0][0], 7n);
    assert.equal(profile.result.transaction.after, 'active');
    assert.ok(profile.metrics.btreeSeeks > 0n);
    assert.equal(typeof profile.metrics.indexSteps, 'bigint');
    assert.deepEqual((await worker.profileSelect('SELECT value FROM docs WHERE value=$value', {$value: 7n})).metrics, profile.metrics);
    await worker.execute('ROLLBACK');
    assert.equal((await worker.exactlyOne('SELECT value FROM docs'))[0], 9223372036854775807n);
    assert.equal((await worker.checkCollectionIntegrity('docs')).documents, 1n);
  } finally { await worker.close(); }
  const reopened = new Database(file);
  try {
    assert.equal(reopened.exactlyOne('SELECT value FROM docs')[0], 9223372036854775807n);
    assert.equal(reopened.checkCollectionIntegrity('docs').indexEntries, 1n);
  }
  finally { reopened.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
`);
  run(process.execPath, ['smoke.cjs'], consumer);
  // Exercise missing/incompatible artifacts only inside the temporary install.
  const installedAddon = path.join(consumer, 'node_modules/@fastdb/node/fastdb.node');
  const savedAddon = installedAddon + '.saved';
  fs.renameSync(installedAddon, savedAddon);
  try {
    const failureProbe = `const assert = require('node:assert/strict');
      assert.throws(()=>require('@fastdb/node'), error => {
        assert.equal(error.code, 'FDB_NATIVE_LOAD');
        assert(error.cause instanceof Error);
        assert(error.message.includes(process.platform + '/' + process.arch));
        assert(error.message.includes('check-node.sh'));
        return true;
      });`;
    run(process.execPath, ['-e', failureProbe], consumer);
    fs.writeFileSync(installedAddon, 'invalid native addon');
    run(process.execPath, ['-e', failureProbe], consumer);
  } finally {
    fs.rmSync(installedAddon, {force:true});
    fs.renameSync(savedAddon, installedAddon);
  }

  // Check declaration resolution from the installed package, with the local
  // compiler as a tool only; the package has no runtime registry dependencies.
  fs.writeFileSync(path.join(consumer, 'smoke.ts'), `import { Database, AsyncDatabase, Record, Vector, VectorComponents, SparseVectorEntry, IntegrityLimits, IntegrityReport, ProfiledQuery } from '@fastdb/node';
const db = new Database();
db.execute('SELECT $id', { $id: new Record('docs', 1n) });
const components: VectorComponents = [1,0,-1] as const;
const vector: Vector = Vector.quantized8(components);
void vector;
const entries: readonly SparseVectorEntry[] = [[0,1],[2,-1]] as const;
Vector.sparse32Entries(3, entries);
// @ts-expect-error sparse values require numbers
Vector.sparse32Entries(3, [[0,1n]]);
// @ts-expect-error components require numbers
Vector.bit1([1n]);
const limits: IntegrityLimits = {maxDocuments: 1n};
const audit: IntegrityReport = db.checkCollectionIntegrity('docs', limits);
const profile: ProfiledQuery = db.profileSelect('SELECT 1');
const counts: bigint[] = [audit.documents, audit.encodedBytes, profile.metrics.vmSteps];
// @ts-expect-error lossless limits require bigint
db.checkCollectionIntegrity('docs', {maxDocuments: 1});
void counts;
db.close();
async function open() {
  const db = await AsyncDatabase.open();
  const audit: IntegrityReport = await db.checkCollectionIntegrity('docs', limits);
  const profile: ProfiledQuery = await db.profileSelect('SELECT 1');
  const counts: bigint[] = [audit.indexEntries, profile.metrics.rowsRead];
  void counts;
  await db.close();
}
void open;
`);
  run(process.execPath, [path.join(packageDir, 'node_modules/typescript/bin/tsc'), '--noEmit', '--strict', '--target', 'ES2022', '--module', 'commonjs', 'smoke.ts'], consumer);
  console.log(`Node package smoke passed: ${process.platform}/${process.arch}, Node ${process.versions.node}, ${packed.entryCount} files, ${packed.size} packed bytes`);
} finally {
  fs.rmSync(temporary, { recursive: true, force: true });
}
