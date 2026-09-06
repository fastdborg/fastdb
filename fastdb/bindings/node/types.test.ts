import { AsyncDatabase, Database, Record, Vector, Value } from './index';
const db = new Database();
const row: Value[] = db.exactlyOne('SELECT $id', { $id: new Record('docs', 1n) });
const rows: Value[][] = db.all('SELECT $blob', { $blob: new Uint8Array([1,2]) });
const changed: bigint = db.execute('SELECT $vector', { $vector: new Vector(new Uint8Array(8)) }).affected;
void row; void rows; void changed;
// @ts-expect-error record keys require explicit string or bigint
new Record('docs', 1);
// @ts-expect-error undefined is not a database value
db.execute('SELECT $x', { $x: undefined });
const migration = db.migrate([{version: 1n, name: 'create', sql: 'CREATE TABLE docs;'}]);
const versions: bigint[] = migration.applied;
const payload: string = db.exportDocuments('docs', 'ndjson');
const count: number = db.importDocuments('docs', payload).imported;
void versions; void count;
// @ts-expect-error versions require bigint
db.migrate([{version: 1, name: 'create', sql: ''}]);
// @ts-expect-error no implicit CSV format
db.exportDocuments('docs', 'csv');
db.close();

async function checkAsync() {
  const asyncDb = await AsyncDatabase.open();
  const result: Value[][] = await asyncDb.all('SELECT 1');
  const exported: string = await asyncDb.exportDocuments('docs');
  void result; void exported;
  await asyncDb.close();
}
void checkAsync;
// @ts-expect-error async construction must wait for native opening
new AsyncDatabase();
async function checkInterrupt() {
  const asyncDb = await AsyncDatabase.open();
  const live: boolean = asyncDb.interrupt();
  void live;
  await asyncDb.close();
}
void checkInterrupt;
