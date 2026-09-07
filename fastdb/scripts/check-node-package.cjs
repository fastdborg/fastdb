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
  async function withWrites(client) {
    await client.execute('BEGIN');
    const changed = await client.execute('WITH chosen AS (SELECT value FROM docs) UPDATE docs SET value=(SELECT $next) WHERE value IN (SELECT value FROM chosen) RETURNING value', {$next:8n});
    assert.deepEqual(changed.rows, [[8n]]);
    assert.equal(changed.affected,1n);
    assert.equal((await client.profileSelect('SELECT record::fetch($id)', {$id:new Record('docs','saved')})).result.rows[0][0].value,8n);
    const removed = await client.execute('WITH chosen AS (SELECT $value AS value) DELETE FROM docs WHERE value IN (SELECT value FROM chosen) RETURNING value', {$value:8n});
    assert.deepEqual(removed.rows,[[8n]]);
    assert.equal((await client.checkCollectionIntegrity('docs')).documents,0n);
    await client.execute('ROLLBACK');
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0],9223372036854775807n);
    await client.execute('BEGIN');
    const nested=await client.execute('UPDATE docs SET value=(SELECT 1) IN (SELECT 1) RETURNING value');
    assert.deepEqual(nested.rows,[[1n]]);
    assert.equal(nested.affected,1n);
    assert.equal((await client.checkCollectionIntegrity('docs')).indexEntries,1n);
    await client.execute('ROLLBACK');
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0],9223372036854775807n);

    await client.execute('BEGIN');
    const correlated=await client.execute('UPDATE docs AS d SET value=(SELECT $next WHERE d.value>$next) RETURNING value',{$next:8n});
    assert.deepEqual(correlated.rows,[[8n]]);
    assert.equal(correlated.affected,1n);
    assert.deepEqual((await client.profileSelect('SELECT (SELECT $n WHERE d.value=$n) FROM docs AS d',{$n:8n})).result.rows,[[8n]]);
    assert.deepEqual(await client.all('SELECT (SELECT 1 WHERE d.value<0) FROM docs AS d'),[[null]]);
    const correlatedDelete=await client.execute('DELETE FROM docs AS d WHERE EXISTS(SELECT 1 WHERE d.value=$n) RETURNING value',{$n:8n});
    assert.deepEqual(correlatedDelete.rows,[[8n]]);
    assert.equal((await client.checkCollectionIntegrity('docs')).indexEntries,0n);
    await client.execute('ROLLBACK');
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0],9223372036854775807n);

    await client.execute('BEGIN');
    const memberUpdate=await client.execute('UPDATE docs AS d SET value=$next WHERE d.value IN(SELECT $current WHERE d.value>0) RETURNING value',{$next:8n,$current:9223372036854775807n});
    assert.deepEqual(memberUpdate.rows,[[8n]]);
    assert.equal(memberUpdate.affected,1n);
    const membership='SELECT d.value IN(SELECT $n WHERE d.value>0),d.value NOT IN(SELECT $n WHERE d.value<0),d.value IN(SELECT NULL WHERE d.value>0) FROM docs AS d';
    assert.deepEqual((await client.profileSelect(membership,{$n:8n})).result.rows,[[1n,1n,null]]);
    assert.equal((await client.checkCollectionIntegrity('docs')).indexEntries,1n);
    await client.execute('ROLLBACK');
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0],9223372036854775807n);

    await client.execute('BEGIN');
    for (const meta of [{deep:{n:8n}},null,7n,[],{}]) {
      await client.execute('UPDATE docs SET meta=$meta',{$meta:meta});
      const expected=meta && !Array.isArray(meta) && typeof meta==='object' && meta.deep ? 8n : null;
      const derived='SELECT d.meta.deep.n FROM (SELECT meta FROM docs) AS d';
      assert.deepEqual(await client.all(derived),[[expected]]);
      const nested='SELECT (SELECT $n WHERE d.meta.deep.n=$n) FROM (SELECT meta FROM docs) AS d';
      assert.deepEqual((await client.profileSelect(nested,{$n:8n})).result.rows,[[expected]]);
    }
    assert.equal((await client.checkCollectionIntegrity('docs')).indexEntries,1n);
    await client.execute('ROLLBACK');
    assert.deepEqual(await client.all('SELECT meta FROM docs'),[[null]]);
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0],9223372036854775807n);

    const cte='WITH docs AS (SELECT 2 AS n) SELECT d.n FROM docs AS d';
    assert.deepEqual(await client.all(cte),[[2n]]);
  }
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
    const fetched = db.profileSelect('SELECT record::fetch($id) AS a,record::fetch($id) AS b', {$id:new Record('docs','saved')});
    assert.equal(fetched.metrics.fetchBatches,1n);
    assert(fetched.metrics.fetchRowsRead>0n);
    assert(fetched.metrics.fetchVmSteps>0n);
    assert.equal(fetched.result.rows[0][0].value,9223372036854775807n);
    assert.deepEqual(fetched.result.rows[0][0],fetched.result.rows[0][1]);
    assert.equal(profile.metrics.fetchBatches,0n);

    for (const make of [Vector.float32, Vector.float64, Vector.sparse32, Vector.quantized8, Vector.bit1, () => Vector.sparse32Entries(3, [[0,1],[2,-1]])]) {
      const vector = make([1,0,-1]);
      assert.deepEqual(db.exactlyOne('SELECT $v AS v', {$v: vector})[0], vector);
    }
    await withWrites(db);
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
    const cancelled = new AbortController(); cancelled.abort();
    const options = {signal:cancelled.signal};
    for (const operation of [
      () => worker.execute('DELETE FROM docs', {}, options),
      () => worker.all('SELECT * FROM docs', {}, options),
      () => worker.first('SELECT * FROM docs', {}, options),
      () => worker.exactlyOne('SELECT * FROM docs', {}, options),
      () => worker.executeBatch('DELETE FROM docs;', options),
      () => worker.profileSelect('SELECT * FROM docs', {}, options),
      () => worker.checkCollectionIntegrity('docs', {}, options),
      () => worker.exportDocuments('docs', 'json', options),
      () => worker.importDocuments('docs', 'invalid', 'ndjson', options),
      () => worker.migrate([], options),
    ]) {
      await assert.rejects(operation(), error => error.code === 'FDB_CANCELLED' && error.transaction.after === 'active');
    }
    assert.equal(require('node:events').getEventListeners(cancelled.signal,'abort').length,0);
    const importSource=new Database();
    let payload;
    try {
      importSource.execute('CREATE TABLE docs');
      importSource.execute('INSERT INTO docs(value) VALUES '+Array.from({length:1000},(_,i)=>'('+(10000+i)+')').join(','));
      payload=importSource.exportDocuments('docs','json');
    } finally {importSource.close();}
    await worker.execute('SAVEPOINT import_check');
    const importing=new AbortController();
    const pendingImport=worker.importDocuments('docs',payload,'json',{signal:importing.signal});
    const timer=setTimeout(()=>importing.abort(),20);
    try {
      await assert.rejects(pendingImport,error=>error.code==='FDB_CANCELLED' && error.transaction.after==='active');
    } finally {clearTimeout(timer);}
    assert.equal((await worker.checkCollectionIntegrity('docs')).documents,1n);
    assert.deepEqual(await worker.exactlyOne('SELECT value FROM docs'),[7n]);
    assert.equal(require('node:events').getEventListeners(importing.signal,'abort').length,0);
    assert.equal((await worker.importDocuments('docs',payload,'json')).imported,1000);
    assert.equal((await worker.checkCollectionIntegrity('docs')).documents,1001n);
    await worker.execute('ROLLBACK TO import_check');
    await worker.execute('RELEASE import_check');
    assert.deepEqual(await worker.exactlyOne('SELECT value FROM docs'),[7n]);
    const fresh = new AbortController();
    const result = await worker.executeBatch('SELECT value FROM docs;', {signal:fresh.signal});
    assert.deepEqual(result[0].result.rows, [[7n]]);
    fresh.abort();
    assert.deepEqual(await worker.exactlyOne('SELECT value FROM docs'), [7n]);

    await assert.rejects(worker.checkCollectionIntegrity('docs', {maxDocuments: 0n}), error => error.code === 'FDB_LIMIT' && error.transaction.after === 'active');
    const profile = await worker.profileSelect('SELECT value FROM docs WHERE value=$value', {$value: 7n});
    assert.equal(profile.result.rows[0][0], 7n);
    assert.equal(profile.result.transaction.after, 'active');
    assert.ok(profile.metrics.btreeSeeks > 0n);
    assert.equal(typeof profile.metrics.indexSteps, 'bigint');
    assert.deepEqual((await worker.profileSelect('SELECT value FROM docs WHERE value=$value', {$value: 7n})).metrics, profile.metrics);

    const fetched = await worker.profileSelect('SELECT record::fetch($id)', {$id:new Record('docs','saved')});
    assert.equal(fetched.result.rows[0][0].value,7n);
    assert.equal(fetched.result.transaction.after,'active');
    assert.equal(fetched.metrics.fetchBatches,1n);
    assert(fetched.metrics.fetchRowsRead>0n);
    assert(fetched.metrics.fetchVmSteps>0n);
    assert.equal(profile.metrics.fetchBatches,0n);
    await worker.execute('ROLLBACK');
    assert.equal((await worker.exactlyOne('SELECT value FROM docs'))[0], 9223372036854775807n);
    assert.equal((await worker.checkCollectionIntegrity('docs')).documents, 1n);
    await withWrites(worker);
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
const counts: bigint[] = [audit.documents, audit.encodedBytes, profile.metrics.vmSteps, profile.metrics.fetchBatches, profile.metrics.fetchRowsRead, profile.metrics.fetchVmSteps];
// @ts-expect-error lossless limits require bigint
db.checkCollectionIntegrity('docs', {maxDocuments: 1});
void counts;
db.close();
async function open() {
  const db = await AsyncDatabase.open();
  const options: import('@fastdb/node').ExecuteOptions = {signal:new AbortController().signal};
  await db.execute('SELECT 1', {}, options);
  await db.all('SELECT 1', {}, options);
  await db.first('SELECT 1', {}, options);
  await db.exactlyOne('SELECT 1', {}, options);
  await db.executeBatch('SELECT 1;', options);
  await db.profileSelect('SELECT 1', {}, options);
  await db.checkCollectionIntegrity('docs', {}, options);
  await db.exportDocuments('docs', 'json', options);
  await db.importDocuments('docs', '', 'ndjson', options);
  await db.migrate([], options);
  // @ts-expect-error cancellation requires an AbortSignal
  await db.migrate([], {signal:true});

  const audit: IntegrityReport = await db.checkCollectionIntegrity('docs', limits);
  const profile: ProfiledQuery = await db.profileSelect('SELECT 1');
  const counts: bigint[] = [audit.indexEntries, profile.metrics.rowsRead, profile.metrics.fetchBatches, profile.metrics.fetchRowsRead, profile.metrics.fetchVmSteps];
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
