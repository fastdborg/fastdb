export class Record { readonly table: string; readonly key: string | bigint; constructor(table: string, key: string | bigint); }
export class Vector { readonly bytes: Uint8Array; constructor(bytes: Uint8Array); }
export type Value = null | boolean | string | bigint | number | Uint8Array | Record | Vector | Value[] | { [field: string]: Value };
export interface Parameters { [name: string]: Value; }
export interface Transaction { before: 'autocommit' | 'active'; after: 'autocommit' | 'active'; }
export interface QueryResult { columns: string[]; rows: Value[][]; affected: bigint; transaction: Transaction; }
export class Database {
  constructor(path?: string);
  close(): void;
  execute(sql: string, parameters?: Parameters): QueryResult;
  all(sql: string, parameters?: Parameters): Value[][];
  first(sql: string, parameters?: Parameters): Value[] | undefined;
  exactlyOne(sql: string, parameters?: Parameters): Value[];
}
