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
    const plan = migrations.map(m => {
      if (typeof m.version !== 'bigint' || typeof m.name !== 'string' || typeof m.sql !== 'string') {
        throw new TypeError('migration requires bigint version, string name and SQL');
      }
      return { version: encode(m.version).value, name: m.name, sql: m.sql };
    });
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
