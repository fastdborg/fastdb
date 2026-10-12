'use strict';
// Open separate copies with immutable old/new packages, never two owning processes.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { execFileSync } = require('node:child_process');
const [oldPackage, newPackage, directory, phase, file] = process.argv.slice(2);
assert.equal(require(path.join(oldPackage, 'package.json')).version, '2.3.0');
assert.equal(require(path.join(newPackage, 'package.json')).version, '2.4.0');
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
  run('upgrade', source); run('old-reader', source); run('reopen', source);
  const restored = path.join(directory, 'restored.db');
  fs.copyFileSync(backup, restored, fs.constants.COPYFILE_EXCL);
  run('old-reopen', restored);
  run('upgrade', restored); run('reopen', restored);
  assert.equal(digest(backup), originalHash);
  const report = { fromVersion: '2.3.0', toVersion: '2.4.0', node: process.version,
    fromAddonSha256: digest(path.join(oldPackage, 'fastdb.node')),
    toAddonSha256: digest(path.join(newPackage, 'fastdb.node')),
    previousBackupSha256: originalHash, backupsUnchanged: true, recordSources: true,
    edgeTables: true, graphTraversal: true, restrictPolicy: true, graphFileRejectedByPrevious: true,
    reopen: true, independentRestore: true };
  fs.writeFileSync(path.join(directory, 'report.json'), JSON.stringify(report, null, 2) + '\n');
} else if (phase === 'old-reader') {
  const { Database } = require(oldPackage);
  assert.throws(() => {
    const db = new Database(file);
    try { db.all('SELECT count(*) FROM people'); } finally { db.close(); }
  }, error => /metadata|version|catalog/i.test(String(error.message)));
} else {
  const { Database } = require(phase === 'seed' || phase === 'old-reopen' ? oldPackage : newPackage);
  const db = new Database(file);
  try {
    if (phase === 'seed') {
      db.execute('CREATE TABLE people');
      db.execute('CREATE UNIQUE INDEX people_name ON people(name)');
      db.execute("INSERT INTO people(id,name) VALUES(people:alice,'Alice'),(people:bob,'Bob'),(people:7,'Seven')");
    } else if (phase === 'old-reopen') {
      assert.deepEqual(db.all('SELECT name FROM people:alice'), [['Alice']]);
    } else {
      assert.deepEqual(db.all('SELECT name FROM people:alice'), [['Alice']]);
      assert.deepEqual(db.all('SELECT name FROM people:7'), [['Seven']]);
      assert.equal(db.checkCollectionIntegrity('people').documents, 3n);
      if (phase === 'upgrade') {
        db.execute('BEGIN');
        db.execute('CREATE EDGE TABLE follows FROM people TO people');
        db.execute('RELATE people:alice -> follows:ab -> people:bob {weight:2}');
        db.execute('RELATE people:bob -> follows:b7 -> people:7');
        db.execute('COMMIT');
        assert.throws(() => db.execute('DELETE FROM people WHERE id = people:bob'), e => e.code === 'FDB_CONSTRAINT');
      }
      assert.deepEqual(
        db.all('SELECT g.node, g.depth FROM GRAPH(people:alice -> follows -> people DEPTH 1 TO 3 MODE REACHABLE) g ORDER BY g.depth'),
        [[db.exactlyOne('SELECT id FROM people:bob')[0], 1n], [db.exactlyOne('SELECT id FROM people:7')[0], 2n]]);
      assert.equal(db.checkCollectionIntegrity('follows').documents, 2n);
    }
  } finally {
    // Produce a self-contained offline backup before copying the main file.
    assert.deepEqual(db.all('PRAGMA wal_checkpoint(TRUNCATE)'), [[0n,0n,0n]]);
    db.close();
  }
}
