'use strict';
const { parentPort, workerData } = require('node:worker_threads');
const { NativeDatabase } = require('./fastdb.node');
const db = new NativeDatabase(workerData.path);
const methods = new Set(['execute', 'profileSelect', 'checkCollectionIntegrity', 'executeBatch', 'migrate', 'exportDocuments', 'importDocuments', 'close']);
parentPort.on('message', ({ id, method, args, cancellationKey }) => {
  try {
    if (!methods.has(method)) throw new Error('unknown database worker operation');
    const result = method === 'execute' && cancellationKey !== undefined
      ? db.executeCancellable(...args, cancellationKey)
      : (method === 'profileSelect' || method === 'checkCollectionIntegrity' || method === 'executeBatch') && cancellationKey !== undefined
        ? db[method](...args, cancellationKey) : db[method](...args);
    parentPort.postMessage({ id, result });
    if (method === 'close') parentPort.close();
  } catch (error) {
    parentPort.postMessage({ id, error: { message: error.message } });
  }
});
parentPort.postMessage({ ready: true, interruptKey: db.interruptKey() });
