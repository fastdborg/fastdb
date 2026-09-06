'use strict';
const { NativeDatabase } = require('./fastdb.node');
class Record {
  constructor(table, key) {
    if (typeof table !== 'string' || !['string','bigint'].includes(typeof key)) throw new TypeError('Record requires a table and string or bigint key');
    this.table = table; this.key = key;
    Object.freeze(this);
  }
}
class Vector {
  constructor(bytes) {
    if (!(bytes instanceof Uint8Array)) throw new TypeError('Vector requires encoded bytes');
    this.bytes = Buffer.from(bytes);
  }
}
function encode(value, depth = 0) {
  if (depth > 64) throw new RangeError('value nesting exceeds 64');
  const tagged = (type, value) => ({ type, value });
  if (value === null) return { type: 'Null' };
  if (typeof value === 'boolean') return tagged('Boolean', value);
  if (typeof value === 'string') return tagged('String', value);
  if (typeof value === 'bigint') {
    if (value < -(1n << 63n) || value >= (1n << 63n)) throw new RangeError('integer outside int64');
    return tagged('Integer', value.toString());
  }
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new TypeError('non-finite number');
    if (Number.isInteger(value) && !Object.is(value, -0)) {
      if (!Number.isSafeInteger(value)) throw new RangeError('use bigint for integers outside the safe Number range');
      return tagged('Integer', String(value));
    }
    const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
    return tagged('Number', bytes.toString('hex'));
  }
  if (value instanceof Record) return tagged('Record', { table: value.table, key: encode(value.key, depth + 1) });
  if (value instanceof Vector) return tagged('Vector', [...value.bytes]);
  if (value instanceof Uint8Array) return tagged('Binary', [...value]);
  if (Array.isArray(value)) return tagged('Array', value.map(v => encode(v, depth + 1)));
  if (value && (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null)) {
    return tagged('Object', Object.fromEntries(Object.entries(value).map(([k,v]) => [k, encode(v, depth + 1)])));
  }
  throw new TypeError('unsupported parameter value');
}
function decode(value) {
  switch (value.type) {
    case 'Null': return null;
    case 'Boolean': case 'String': return value.value;
    case 'Integer': return BigInt(value.value);
    case 'Number': return Buffer.from(value.value, 'hex').readDoubleBE();
    case 'Binary': return Buffer.from(value.value);
    case 'Vector': return new Vector(Buffer.from(value.value));
    case 'Record': return new Record(value.value.table, decode(value.value.key));
    case 'Array': return value.value.map(decode);
    case 'Object': return Object.fromEntries(Object.entries(value.value).map(([k,v]) => [k, decode(v)]));
    default: throw new Error('unsupported native value encoding');
  }
}
function unwrap(raw) {
  const report = JSON.parse(raw);
  if (report.version !== 1) throw new Error('unsupported native report version');
  if (report.execution.error) {
    const error = new Error(report.execution.error.message);
    error.code = report.execution.error.code;
    error.transaction = report.transaction;
    throw error;
  }
  return report;
}
function migrationPlan(migrations) {
  const plan = migrations.map(m => {
      if (typeof m.version !== 'bigint' || typeof m.name !== 'string' || typeof m.sql !== 'string') {
        throw new TypeError('migration requires bigint version, string name and SQL');
      }
      return { version: encode(m.version).value, name: m.name, sql: m.sql };
    });
  return plan;
}
class Database {
  #native;
  constructor(path = ':memory:') { this.#native = new NativeDatabase(path); }
  close() { this.#native.close(); }
  execute(sql, parameters = {}) {
    const params = Object.fromEntries(Object.entries(parameters).map(([k,v]) => [k, encode(v)]));
    const report = unwrap(this.#native.execute(sql, JSON.stringify(params)));
    const result = report.execution.result;
    return { columns: result.columns, rows: result.rows.map(row => row.map(decode)), affected: BigInt(result.affected), transaction: report.transaction };
  }
  exportDocuments(table, format = 'json') {
    return unwrap(this.#native.exportDocuments(table, format)).execution.result;
  }
  importDocuments(table, input, format = 'json') {
    const report = unwrap(this.#native.importDocuments(table, input, format));
    return { ...report.execution.result, transaction: report.transaction };
  }
  migrate(migrations) {
    const plan = migrationPlan(migrations);
    const report = unwrap(this.#native.migrate(JSON.stringify(plan)));
    return { alreadyApplied: report.execution.result.alreadyApplied,
      applied: report.execution.result.applied.map(BigInt), transaction: report.transaction };
  }
  all(sql, parameters) { return this.execute(sql, parameters).rows; }
  first(sql, parameters) { return this.all(sql, parameters)[0]; }
  exactlyOne(sql, parameters) {
    const rows = this.all(sql, parameters);
    if (rows.length !== 1) throw new RangeError(`expected exactly one row, got ${rows.length}`);
    return rows[0];
  }
}
exports.Database = Database;
exports.Record = Record;
exports.Vector = Vector;


const asyncConstruction = Symbol('AsyncDatabase');
class AsyncDatabase {
  #worker; #pending = new Map(); #next = 0; #bytes = 0;
  #ready; #readyResolve; #readyReject; #exited;
  #failure; #closing = false; #closePromise;
  constructor(path, token) {
    if (token !== asyncConstruction) throw new TypeError('use AsyncDatabase.open()');
    const { Worker } = require('node:worker_threads');
    this.#ready = new Promise((resolve, reject) => { this.#readyResolve = resolve; this.#readyReject = reject; });
    this.#worker = new Worker(require.resolve('./worker.cjs'), { workerData: { path } });
    this.#exited = new Promise(resolve => this.#worker.once('exit', code => {
      if (!this.#closing || this.#pending.size) this.#fail(new Error(`database worker exited (${code})`));
      resolve();
    }));
    this.#worker.on('error', error => this.#fail(error));
    this.#worker.on('messageerror', error => this.#fail(error));
    this.#worker.on('message', message => {
      if (message.ready) { this.#readyResolve(); return; }
      const pending = this.#pending.get(message.id);
      if (!pending) return;
      this.#pending.delete(message.id); this.#bytes -= pending.bytes;
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
    });
  }
  static async open(path = ':memory:') {
    const db = new AsyncDatabase(path, asyncConstruction);
    try { await db.#ready; return db; }
    catch (error) { await db.#exited; throw error; }
  }
  #fail(error) {
    this.#failure = error;
    this.#readyReject(error);
    for (const pending of this.#pending.values()) pending.reject(error);
    this.#pending.clear(); this.#bytes = 0;
  }
  #request(method, args, closing = false) {
    if (this.#failure) return Promise.reject(this.#failure);
    if (this.#closing && !closing) return Promise.reject(new Error('database is closing or closed'));
    const bytes = args.reduce((size, arg) => size + Buffer.byteLength(arg), 0);
    if (!closing && (this.#pending.size >= 256 || this.#bytes + bytes > 128 * 1024 * 1024)) {
      const error = new RangeError('database worker queue limit exceeded'); error.code = 'FDB_LIMIT';
      return Promise.reject(error);
    }
    const id = ++this.#next;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject, bytes }); this.#bytes += bytes;
      try { this.#worker.postMessage({ id, method, args }); }
      catch (error) { this.#pending.delete(id); this.#bytes -= bytes; reject(error); }
    });
  }
  close() {
    if (!this.#closePromise) {
      this.#closing = true;
      this.#closePromise = this.#request('close', [], true).then(() => this.#exited);
    }
    return this.#closePromise;
  }
  async execute(sql, parameters = {}) {
    const params = Object.fromEntries(Object.entries(parameters).map(([k,v]) => [k, encode(v)]));
    const report = unwrap(await this.#request('execute', [sql, JSON.stringify(params)]));
    const result = report.execution.result;
    return { columns: result.columns, rows: result.rows.map(row => row.map(decode)), affected: BigInt(result.affected), transaction: report.transaction };
  }
  async exportDocuments(table, format = 'json') {
    return unwrap(await this.#request('exportDocuments', [table, format])).execution.result;
  }
  async importDocuments(table, input, format = 'json') {
    const report = unwrap(await this.#request('importDocuments', [table, input, format]));
    return { ...report.execution.result, transaction: report.transaction };
  }
  async migrate(migrations) {
    const report = unwrap(await this.#request('migrate', [JSON.stringify(migrationPlan(migrations))]));
    return { alreadyApplied: report.execution.result.alreadyApplied,
      applied: report.execution.result.applied.map(BigInt), transaction: report.transaction };
  }
  async all(sql, parameters) { return (await this.execute(sql, parameters)).rows; }
  async first(sql, parameters) { return (await this.all(sql, parameters))[0]; }
  async exactlyOne(sql, parameters) {
    const rows = await this.all(sql, parameters);
    if (rows.length !== 1) throw new RangeError(`expected exactly one row, got ${rows.length}`);
    return rows[0];
  }
}
exports.AsyncDatabase = AsyncDatabase;
