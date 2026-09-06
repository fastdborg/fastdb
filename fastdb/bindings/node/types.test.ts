import { Database, Record, Vector, Value } from './index';
const db = new Database();
const row: Value[] = db.exactlyOne('SELECT $id', { $id: new Record('docs', 1n) });
const rows: Value[][] = db.all('SELECT $blob', { $blob: new Uint8Array([1,2]) });
const changed: bigint = db.execute('SELECT $vector', { $vector: new Vector(new Uint8Array(8)) }).affected;
void row; void rows; void changed;
// @ts-expect-error record keys require explicit string or bigint
new Record('docs', 1);
// @ts-expect-error undefined is not a database value
db.execute('SELECT $x', { $x: undefined });
db.close();
