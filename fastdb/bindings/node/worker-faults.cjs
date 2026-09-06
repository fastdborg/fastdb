'use strict';
// Isolated test process: replace only the Worker transport. Native query tests
// remain in test.cjs; this fixture does not claim native crash recovery.
const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const threads = require('node:worker_threads');
let latest;
class FaultWorker extends EventEmitter {
  messages = []; stopped = false; rejectClose = false;
  constructor() {
    super(); latest = this;
    queueMicrotask(() => this.emit('message', { ready: true, interruptKey: '0' }));
  }
  postMessage(message) {
    this.messages.push(message);
    if (message.method === 'close') {
      if (this.rejectClose) throw new Error('transport closed');
      queueMicrotask(() => {
        this.emit('message', { id: message.id });
        this.stopped = true; this.emit('exit', 0);
      });
    }
  }
  async terminate() { this.stopped = true; this.emit('exit', 1); return 1; }
}
threads.Worker = FaultWorker;
const { AsyncDatabase } = require('./index.cjs');
(async () => {
  for (const brokenSend of [false, true]) {
    const db = await AsyncDatabase.open();
    const worker = latest; worker.rejectClose = brokenSend;
    const first = db.execute('INSERT INTO example VALUES (1)');
    const second = db.execute('SELECT 1');
    const settled = Promise.allSettled([first, second]);
    const cause = new Error('injected message decoding failure');
    worker.emit('messageerror', cause);
    const outcomes = await settled;
    assert(outcomes.every(r => r.status === 'rejected' && r.reason.code === 'FDB_WORKER'));
    assert.equal(outcomes[0].reason, outcomes[1].reason);
    assert.equal(outcomes[0].reason.cause, cause);
    const close = db.close(); assert.equal(db.close(), close); await close;
    assert.equal(worker.stopped, true);
    assert.equal(worker.messages.filter(m => m.method === 'close').length, 1);
    await assert.rejects(db.execute('SELECT 2'), error => error === outcomes[0].reason);
  }
  {
    const db = await AsyncDatabase.open(); const worker = latest;
    const pending = db.execute('SELECT 1');
    const rejected = assert.rejects(pending, error => error.code === 'FDB_WORKER' && error.message.includes('original worker error'));
    worker.emit('error', new Error('original worker error'));
    worker.stopped = true; worker.emit('exit', 1);
    await rejected; await db.close();
  }
  {
    const db = await AsyncDatabase.open(); const worker = latest;
    const pending = db.execute('SELECT 1');
    const rejected = assert.rejects(pending, error => error.code === 'FDB_WORKER');
    worker.stopped = true; worker.emit('exit', 7);
    await rejected; await db.close();
  }
  {
    const db = await AsyncDatabase.open(); const worker = latest;
    const close = db.close();
    const rejected = assert.rejects(close, error => error.code === 'FDB_WORKER');
    worker.emit('messageerror', new Error('lost close acknowledgement'));
    await rejected;
    assert.equal(worker.stopped, true);
  }
  process.stdout.write('worker-faults-complete\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
