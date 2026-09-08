export class Record { readonly table: string; readonly key: string | bigint; constructor(table: string, key: string | bigint); }
export type SparseVectorEntry = readonly [index: number, value: number];
export type VectorComponents = readonly number[] | Float32Array | Float64Array;
export class Vector {
  readonly bytes: Uint8Array;
  constructor(bytes: Uint8Array);
  static float32(values: VectorComponents): Vector;
  static float64(values: VectorComponents): Vector;
  static sparse32(values: VectorComponents): Vector;
  static sparse32Entries(dimensions: number, entries: readonly SparseVectorEntry[]): Vector;
  static quantized8(values: VectorComponents): Vector;
  static bit1(values: VectorComponents): Vector;
}
export type Value = null | boolean | string | bigint | number | Uint8Array | Record | Vector | Value[] | { [field: string]: Value };
export interface Parameters { [name: string]: Value; }
export interface Transaction { before: 'autocommit' | 'active'; after: 'autocommit' | 'active'; }
export interface FastDBError extends Error { code: string; transaction?: Transaction; }
/** Recognizes coded FastDB errors and validates any transaction observations. */
export function isFastDBError(value: unknown): value is FastDBError;
export interface QueryResult { columns: string[]; rows: Value[][]; affected: bigint; transaction: Transaction; }
/** Retained logical result limits; excludes engine working memory and temporary decoding. */
export interface ResultLimits { maxRows: bigint; maxPayloadBytes: bigint; }
export interface IntegrityLimits { maxDocuments?: bigint; maxEncodedBytes?: bigint; }
export interface IntegrityReport {
  documents: bigint; indexes: bigint; indexEntries: bigint; encodedBytes: bigint; transaction: Transaction;
}
export interface QueryMetrics {
  rowsRead: bigint; rowsWritten: bigint; fullscanSteps: bigint; indexSteps: bigint;
  vmSteps: bigint; sortOperations: bigint; btreeSeeks: bigint;
  fetchBatches: bigint; fetchRowsRead: bigint; fetchVmSteps: bigint;
}
export interface ProfiledQuery { result: QueryResult; metrics: QueryMetrics; }
export type BatchExecution = { offset: number; transaction: Transaction } & (
  { result: Omit<QueryResult, 'transaction'>; error?: never } |
  { error: { code: string; message: string }; result?: never }
);
export type TransferFormat = 'json' | 'ndjson';
export interface Migration { version: bigint; name: string; sql: string; }
export interface MigrationReport { alreadyApplied: number; applied: bigint[]; transaction: Transaction; }
export interface ImportReport { imported: number; transaction: Transaction; }
export class Database {
  constructor(path?: string);
  close(): void;
  migrate(migrations: Migration[]): MigrationReport;
  exportDocuments(table: string, format?: TransferFormat): string;
  importDocuments(table: string, input: string, format?: TransferFormat): ImportReport;
  execute(sql: string, parameters?: Parameters): QueryResult;
  profileSelect(sql: string, parameters?: Parameters): ProfiledQuery;
  selectWithLimits(sql: string, limits: ResultLimits, parameters?: Parameters): QueryResult;
  profileSelectWithLimits(sql: string, limits: ResultLimits, parameters?: Parameters): ProfiledQuery;
  checkCollectionIntegrity(table: string, limits?: IntegrityLimits): IntegrityReport;
  executeBatch(script: string): BatchExecution[];
  all(sql: string, parameters?: Parameters): Value[][];
  first(sql: string, parameters?: Parameters): Value[] | undefined;
  exactlyOne(sql: string, parameters?: Parameters): Value[];
}

export interface ExecuteOptions { signal?: AbortSignal; }
export class AsyncDatabase {
  private constructor();
  static open(path?: string): Promise<AsyncDatabase>;
  interrupt(): boolean;
  close(): Promise<void>;
  migrate(migrations: Migration[], options?: ExecuteOptions): Promise<MigrationReport>;
  exportDocuments(table: string, format?: TransferFormat, options?: ExecuteOptions): Promise<string>;
  importDocuments(table: string, input: string, format?: TransferFormat, options?: ExecuteOptions): Promise<ImportReport>;
  execute(sql: string, parameters?: Parameters, options?: ExecuteOptions): Promise<QueryResult>;
  profileSelect(sql: string, parameters?: Parameters, options?: ExecuteOptions): Promise<ProfiledQuery>;
  selectWithLimits(sql: string, limits: ResultLimits, parameters?: Parameters, options?: ExecuteOptions): Promise<QueryResult>;
  profileSelectWithLimits(sql: string, limits: ResultLimits, parameters?: Parameters, options?: ExecuteOptions): Promise<ProfiledQuery>;
  checkCollectionIntegrity(table: string, limits?: IntegrityLimits, options?: ExecuteOptions): Promise<IntegrityReport>;
  executeBatch(script: string, options?: ExecuteOptions): Promise<BatchExecution[]>;
  all(sql: string, parameters?: Parameters, options?: ExecuteOptions): Promise<Value[][]>;
  first(sql: string, parameters?: Parameters, options?: ExecuteOptions): Promise<Value[] | undefined>;
  exactlyOne(sql: string, parameters?: Parameters, options?: ExecuteOptions): Promise<Value[]>;
}
