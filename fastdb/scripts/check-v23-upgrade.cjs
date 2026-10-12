'use strict';
// Open separate copies with immutable old/new packages, never two owning processes.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { execFileSync } = require('node:child_process');
const [oldPackage, newPackage, directory, phase, file] = process.argv.slice(2);
assert.equal(require(path.join(oldPackage, 'package.json')).version, '2.2.0');
const newVersion = require(path.join(newPackage, 'package.json')).version;
assert.ok(['2.3.0', '2.4.0'].includes(newVersion), newVersion);
const digest = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
if (!phase) {
  fs.mkdirSync(directory);
  const source = path.join(directory, 'source.db');
  const backup = path.join(directory, 'previous-backup.db');
  const run = (step, database) => execFileSync(process.execPath,
    [__filename, oldPackage, newPackage, directory, step, database], { stdio: 'inherit', timeout: 120000 });
  run('seed', source);
  fs.copyFileSync(source, backup, fs.constants.COPYFILE_EXCL);
  const originalHash = digest(backup);
  run('upgrade', source); run('reopen', source);
  const restored = path.join(directory, 'restored.db');
  fs.copyFileSync(backup, restored, fs.constants.COPYFILE_EXCL);
  run('upgrade', restored); run('reopen', restored);
  assert.equal(digest(backup), originalHash);
  const report = { fromVersion: '2.2.0', toVersion: newVersion, node: process.version,
    fromAddonSha256: digest(path.join(oldPackage, 'fastdb.node')),
    toAddonSha256: digest(path.join(newPackage, 'fastdb.node')),
    previousBackupSha256: originalHash, backupsUnchanged: true,
    recordSources: true, typedKeys: true, indexRollback: true, reopen: true, independentRestore: true };
  fs.writeFileSync(path.join(directory, 'report.json'), JSON.stringify(report, null, 2) + '\n');
} else {
  const { Database } = require(phase === 'seed' ? oldPackage : newPackage);
  const db = new Database(file);
  try {
    if (phase === 'seed') {
      db.execute('CREATE TABLE docs');
      db.execute('CREATE UNIQUE INDEX docs_title ON docs(title)');
      db.execute("INSERT INTO docs(id,title) VALUES(docs:a,'old'),(docs:7,'integer'),(type::record('docs','7'),'string')");
    } else {
      assert.deepEqual(db.all('SELECT title FROM docs:a'), [[phase === 'upgrade' ? 'old' : 'new']]);
      assert.deepEqual(db.all('SELECT title FROM docs:7'), [['integer']]);
      assert.deepEqual(db.all("SELECT title FROM type::record('docs', $key)", { $key: '7' }), [['string']]);
      assert.deepEqual(db.all('SELECT title FROM docs:missing'), []);
      assert.equal(db.checkCollectionIntegrity('docs').documents, 3n);
      if (phase === 'upgrade') {
        db.execute('BEGIN');
        assert.throws(() => db.execute("UPDATE docs:a SET title='integer'"), e => e.code === 'FDB_CONSTRAINT');
        assert.deepEqual(db.all('SELECT title FROM docs:a'), [['old']]);
        db.execute("UPDATE docs:a SET title='new'"); db.execute('COMMIT');
      }
    }
  } finally {
    // Produce a self-contained offline backup before copying the main file.
    assert.deepEqual(db.all('PRAGMA wal_checkpoint(TRUNCATE)'), [[0n,0n,0n]]);
    db.close();
  }
}
