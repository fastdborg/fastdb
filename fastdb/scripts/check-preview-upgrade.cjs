'use strict';
// Run with absolute paths to separately installed old/new @fastdb/node packages.
// Each phase runs in a separate process so the native binaries never share a VM.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {execFileSync} = require('node:child_process');
const [oldPackage, newPackage, phase, file] = process.argv.slice(2);
if (!oldPackage || !newPackage) throw new Error('Expected old and new installed package paths');
if (!phase) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'fastdb-upgrade-'));
  try {
    for (const step of ['old', 'new', 'reopen']) {
      execFileSync(process.execPath, [__filename, oldPackage, newPackage, step, path.join(dir, 'app.db')], {stdio:'inherit'});
    }
    console.log('Released-binary upgrade and reopen passed');
  } finally { fs.rmSync(dir, {recursive:true, force:true}); }
} else {
  const {Database} = require(phase === 'old' ? oldPackage : newPackage);
  const db = new Database(file);
  try {
    if (phase === 'old') {
      db.execute('CREATE TABLE users');
      db.execute('CREATE UNIQUE INDEX user_email ON users(email)');
      db.execute("INSERT INTO users {id:users:alice,email:'alice@example.test',name:'Alice'}");
      db.execute('CREATE TABLE events(id INTEGER PRIMARY KEY, message TEXT)');
      db.execute("INSERT INTO events VALUES(1,'created')");
    } else {
      assert.deepEqual(db.all('SELECT name FROM users WHERE email=$email', {$email:'alice@example.test'}), [['Alice']]);
      assert.deepEqual(db.all('SELECT message FROM events WHERE id=1'), [['created']]);
      db.checkCollectionIntegrity('users');
      if (phase === 'new') {
        db.execute('BEGIN');
        db.execute("UPDATE users SET name='Temporary'");
        db.execute("INSERT INTO events VALUES(2,'temporary')");
        db.execute('ROLLBACK');
        assert.throws(()=>db.execute("INSERT INTO users {id:users:duplicate,email:'alice@example.test'}"));
        assert.equal(db.all('SELECT * FROM users').length, 1);
        db.execute("UPSERT notes:first {body:'Created after upgrade'}");
      } else {
        assert.equal(db.all('SELECT * FROM events').length, 1);
        assert.deepEqual(db.all('SELECT body FROM notes'), [['Created after upgrade']]);
        db.checkCollectionIntegrity('notes');
      }
    }
    console.log(`${phase}: passed`);
  } finally { db.close(); }
}
