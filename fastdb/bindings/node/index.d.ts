export class Record { readonly table: string; readonly key: string | bigint; constructor(table: string, key: string | bigint); }
export class Vector { readonly bytes: Uint8Array; constructor(bytes: Uint8Array); }
export type Value = null | boolean | string | bigint | number | Uint8Array | Record | Vector | Value[] | { [field: string]: Value };
export interface Parameters { [name: string]: Value; }
export interface Transaction { before: 'autocommit' | 'active'; after: 'autocommit' | 'active'; }
export interface QueryResult { columns: string[]; rows: Value[][]; affected: bigint; transaction: Transaction; }
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
  all(sql: string, parameters?: Parameters): Value[][];
  first(sql: string, parameters?: Parameters): Value[] | undefined;
  exactlyOne(sql: string, parameters?: Parameters): Value[];
}
