'use strict';
// Open separate copies with immutable old/new packages, never two owning processes.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { execFileSync } = require('node:child_process');
const [oldPackage, newPackage, directory, phase, file] = process.argv.slice(2);
assert.equal(require(path.join(oldPackage, 'package.json')).version, '2.4.0');
const newVersion = require(path.join(newPackage, 'package.json')).version;
require('./versions.cjs').assertCandidate(newVersion, '2.5.0');
const digest = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const point = (x, y) => ({ type: 'Point', coordinates: [x, y] });
if (!phase) {
  fs.mkdirSync(directory);
  const source = path.join(directory, 'source.db');
  const backup = path.join(directory, 'previous-backup.db');
  const run = (step, database) => execFileSync(process.execPath,
    [__filename, oldPackage, newPackage, directory, step, database], { stdio: 'inherit', timeout: 120000 });
  run('seed', source);
  fs.copyFileSync(source, backup, fs.constants.COPYFILE_EXCL);
  const originalHash = digest(backup);
  run('latitude-format', source);
  run('old-reopen', source);
  run('upgrade', source); run('old-reader', source); run('reopen', source);
  const restored = path.join(directory, 'restored.db');
  fs.copyFileSync(backup, restored, fs.constants.COPYFILE_EXCL);
  run('old-reopen', restored);
  run('upgrade', restored); run('reopen', restored);
  assert.equal(digest(backup), originalHash);
  const report = { fromVersion: '2.4.0', toVersion: newVersion, node: process.version,
    fromAddonSha256: digest(path.join(oldPackage, 'fastdb.node')),
    toAddonSha256: digest(path.join(newPackage, 'fastdb.node')),
    previousBackupSha256: originalHash, backupsUnchanged: true,
    latitudeIndexServesNewSourcesWithoutRewrite: true, reindexUpgradesToTwoDimensional: true,
    regionIndex: true, catalogVersion7RejectedByPrevious: true, reopen: true, independentRestore: true };
  fs.writeFileSync(path.join(directory, 'report.json'), JSON.stringify(report, null, 2) + '\n');
} else if (phase === 'old-reader') {
  const { Database } = require(oldPackage);
  assert.throws(() => {
    const db = new Database(file);
    try { db.all('SELECT count(*) FROM places'); } finally { db.close(); }
  }, error => /metadata|version|catalog/i.test(String(error.message)));
} else {
  const { Database, Record } = require(phase === 'seed' || phase === 'old-reopen' ? oldPackage : newPackage);
  const db = new Database(file);
  const near = () => db.all("SELECT id FROM search::near('places_location', geo::point(106.7, 10.8), 3000) ORDER BY id")
    .map(row => String(row[0].key)).sort();
  // Reference: the released haversine on the 6,371,008.8 m sphere over every stored point.
  const expectedNear = () => {
    const rad = Math.PI / 180;
    return db.all('SELECT id, location FROM places').filter(([, location]) => {
      const [x, y] = location.coordinates.map(v => (v !== null && typeof v === 'object' ? v.value : Number(v)));
      const h = Math.sin((y - 10.8) * rad / 2) ** 2 + Math.cos(10.8 * rad) * Math.cos(y * rad) * Math.sin((x - 106.7) * rad / 2) ** 2;
      return 2 * 6371008.8 * Math.atan2(Math.sqrt(h), Math.sqrt(1 - h)) <= 3000;
    }).map(([id]) => String(id.key)).sort();
  };
  try {
    if (phase === 'seed') {
      db.execute('CREATE TABLE places');
      db.execute('BEGIN');
      for (let i = 0; i < 40; i++) {
        db.execute('INSERT INTO places DOCUMENT $d', { $d: { id: new Record('places', BigInt(i)), open: i % 2 === 0,
          location: point(106.7 + (i % 8) * 0.01, 10.8 + Math.floor(i / 8) * 0.01) } });
      }
      db.execute('COMMIT');
      db.execute('CREATE SEARCH INDEX places_location ON places(location) USING SPATIAL');
    } else if (phase === 'old-reopen') {
      assert.equal(db.checkCollectionIntegrity('places').documents, 40n);
      assert.deepEqual(near(), expectedNear());
      assert.ok(near().length >= 5);
    } else if (phase === 'latitude-format') {
      // 2.5.0 serves every source from the released latitude format without
      // rewriting it, so 2.4.0 can still open the file afterwards.
      assert.deepEqual(near(), expectedNear());
      assert.ok(near().length >= 5);
      assert.equal(db.all("SELECT id FROM search::bbox('places_location', 106.7, 10.8, 106.72, 10.81)").length, 6);
      assert.deepEqual(db.all("SELECT id FROM search::nearest('places_location', ST_Point(106.7, 10.8), 1)").map(r => String(r[0].key)), ['0']);
      assert.equal(db.all('SELECT id FROM places WHERE ST_Covers(ST_MakeEnvelope(106.69, 10.79, 106.715, 10.815), location)').length, 4);
      assert.equal(db.checkCollectionIntegrity('places').documents, 40n);
    } else {
      assert.deepEqual(near(), expectedNear());
      assert.ok(near().length >= 5);
      if (phase === 'upgrade') {
        db.execute('REINDEX places_location');
        db.execute('CREATE TABLE zones');
        db.execute('INSERT INTO zones {id:zones:north,shape:ST_MakeEnvelope(106.69, 10.82, 106.8, 10.9)}');
        db.execute("CREATE SEARCH INDEX zones_shape ON zones(shape) USING SPATIAL WITH (geometry = 'region')");
        db.execute('UPDATE places SET location = geo::point(106.75, 10.85) WHERE id = places:0');
      }
      assert.deepEqual(db.all("SELECT id FROM search::containing('zones_shape', ST_Point(106.75, 10.85), 'covers')").map(r => String(r[0].key)), ['north']);
      assert.equal(db.all('SELECT id FROM places WHERE ST_DWithin(location, ST_Point(106.75, 10.85), 10) AND open = true').length, 1);
      assert.deepEqual(db.all('SELECT id FROM places WHERE ST_DistanceSphere(location, ST_Point(106.7, 10.8)) IS NOT NULL ORDER BY ST_DistanceSphere(location, ST_Point(106.7, 10.8)), id LIMIT 2')
        .map(r => String(r[0].key)), ['1', '8']);
      assert.equal(db.checkCollectionIntegrity('places').documents, 40n);
      assert.equal(db.checkCollectionIntegrity('zones').documents, 1n);
    }
  } finally {
    // Produce a self-contained offline backup before copying the main file.
    assert.deepEqual(db.all('PRAGMA wal_checkpoint(TRUNCATE)'), [[0n,0n,0n]]);
    db.close();
  }
}
