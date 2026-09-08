'use strict';
// Maintainer smoke: build with check-node.sh first. No publishing or registry
// access; all artifacts and the consumer installation live in a temp directory.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const packageDir = path.resolve(__dirname, '../bindings/node');
const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
assert(fs.existsSync(path.join(packageDir, 'fastdb.node')), 'Build the addon with fastdb/scripts/check-node.sh first');
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'fastdb-package-'));
const run = (command, args, cwd) => execFileSync(command, args, {
  cwd, encoding: 'utf8', timeout: 120000, maxBuffer: 2 * 1024 * 1024,
  env: { ...process.env, NODE_PATH: '' },
});
try {
  const [packed] = JSON.parse(run(npm, ['pack', '--offline', '--ignore-scripts', '--json', '--pack-destination', temporary], packageDir));
  assert.deepEqual(packed.files.map(file => file.path).sort(), [
    'LICENSE.md', 'THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_CRATE_NOTICES.md', 'README.md', 'fastdb.node', 'index.cjs', 'index.d.ts', 'package.json', 'worker.cjs', 'native.cjs',
  ].sort());
  assert(packed.files.find(file => file.path === 'fastdb.node').size > 0);
  const consumer = path.join(temporary, 'consumer');
  fs.mkdirSync(consumer);
  fs.writeFileSync(path.join(consumer, 'package.json'), JSON.stringify({ name: 'fastdb-package-smoke', version: '0.0.0', private: true }));
  run(npm, ['install', '--offline', '--ignore-scripts', '--no-audit', '--no-fund', '--package-lock=false', path.join(temporary, packed.filename)], consumer);
  for (const notice of ['THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_CRATE_NOTICES.md']) {
    assert.equal(fs.readFileSync(path.join(consumer, 'node_modules/@fastdb/node', notice), 'utf8'), fs.readFileSync(path.join(packageDir, notice), 'utf8'));
  }
  fs.writeFileSync(path.join(consumer, 'smoke.cjs'), `
'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const { Database, AsyncDatabase, Record, Vector, isFastDBError } = require('@fastdb/node');
assert(require.resolve('@fastdb/node').startsWith(path.join(__dirname, 'node_modules')));
(async () => {
  async function withVectorFields(client) {
    await client.execute('BEGIN');
    try {
      for (const make of [Vector.float32, Vector.float64, Vector.sparse32, Vector.quantized8, Vector.bit1]) {
        const vector = make([1,0,-1]);
        await client.execute('UPDATE docs SET embedding=$v', {$v:vector});
        const expected = await client.exactlyOne('SELECT vector_extract($v) AS v', {$v:vector});
        const actual = await client.profileSelect('SELECT vector_extract(embedding) AS v FROM docs');
        assert.deepEqual(actual.result.rows, [expected]);
        assert.equal(actual.metrics.rowsWritten, 0n);
        assert.deepEqual(await client.exactlyOne('SELECT embedding FROM docs'), [vector]);
        const budget = {maxRows:1n,maxPayloadBytes:9n+BigInt(vector.bytes.length)};
        assert.deepEqual((await client.selectWithLimits('SELECT embedding FROM docs',budget)).rows, [[vector]]);
        assert.deepEqual((await client.profileSelectWithLimits('SELECT embedding FROM docs',budget)).result.rows, [[vector]]);
        await assert.rejects(async () => client.selectWithLimits('SELECT embedding FROM docs', {...budget,maxPayloadBytes:budget.maxPayloadBytes-1n}), error => {
          assert.equal(error.code,'FDB_LIMIT');
          assert.deepEqual(error.transaction,{before:'active',after:'active'});
          return true;
        });
        assert.equal((await client.checkCollectionIntegrity('docs')).documents, 1n);
      }
      assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0], 9223372036854775807n);
    } finally {
      await client.execute('ROLLBACK');
    }
    assert.equal((await client.exactlyOne('SELECT embedding FROM docs'))[0], null);
    await assert.rejects(async () => client.all('SELECT vector_extract(embedding) FROM docs'));
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0], 9223372036854775807n);
  }
  async function withDuplicateColumns(client) {
    await client.execute('BEGIN');
    try {
      await client.execute('CREATE TABLE budget_targets(id INTEGER PRIMARY KEY,n TEXT)');
      await client.execute("INSERT INTO budget_targets VALUES(1,'猫')");
      const fetchSql = 'SELECT record::fetch(budget_targets:1) AS v,record::fetch(budget_targets:1) AS w';
      const fetchBudget = {maxRows:1n,maxPayloadBytes:30n};
      const fetched = await client.profileSelect(fetchSql);
      assert.deepEqual((await client.selectWithLimits(fetchSql,fetchBudget)).rows,fetched.result.rows);
      assert.deepEqual((await client.profileSelectWithLimits(fetchSql,fetchBudget)).result,fetched.result);
      await assert.rejects(async () => client.selectWithLimits(fetchSql,{...fetchBudget,maxPayloadBytes:29n}), error => {
        assert.equal(error.code,'FDB_LIMIT');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      const writeSql = "INSERT INTO budget_targets VALUES(2,'é'),(3,'猫') RETURNING n";
      const writeBudget = {maxRows:2n,maxPayloadBytes:6n};
      await assert.rejects(async () => client.writeWithResultLimits(writeSql,{...writeBudget,maxPayloadBytes:5n}), error => {
        assert.equal(error.code,'FDB_LIMIT');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      assert.deepEqual(await client.all('SELECT id FROM budget_targets'),[[1n]]);
      assert.deepEqual((await client.writeWithResultLimits(writeSql,writeBudget)).rows,[['é'],['猫']]);
      const bytes = Buffer.from([0,255,49]);
      const typed = 'WITH q(x,x) AS (SELECT $flag,$bytes FROM docs), r AS (SELECT q.* FROM q) SELECT r.* FROM r';
      const result = await client.execute(typed, {$flag:true,$bytes:bytes});
      assert.deepEqual(result.columns, ['x','x']);
      assert.deepEqual(result.rows, [[true,bytes]]);
      const folded = await client.execute('WITH q(x,X) AS (SELECT $flag,$bytes FROM docs) SELECT q.* FROM q', {$flag:true,$bytes:bytes});
      assert.deepEqual(folded.columns, ['x','x']);
      assert.deepEqual(folded.rows, result.rows);
      const using = await client.execute('WITH q("Key") AS (SELECT value FROM docs), r("Key") AS (SELECT 9223372036854775807) SELECT "Key",(SELECT "Key" ORDER BY "Key") AS correlated FROM q a RIGHT JOIN r b USING("Key")');
      assert.deepEqual(using.columns, ['key','correlated']);
      assert.deepEqual(using.rows, [[9223372036854775807n,9223372036854775807n]]);
      const natural = await client.execute('WITH q("Key") AS (SELECT value FROM docs), r("Key") AS (SELECT 9223372036854775807) SELECT "Key",(SELECT (SELECT "Key")) AS correlated FROM q a NATURAL RIGHT JOIN r b');
      assert.deepEqual(natural.columns, using.columns);
      assert.deepEqual(natural.rows, using.rows);
      const derived = await client.execute('WITH q("Key") AS (SELECT value FROM docs), r("Key") AS (SELECT 9223372036854775807) SELECT "Key",(SELECT (SELECT "Key" FROM (SELECT 1 AS n) nums WHERE n=1)) AS correlated FROM q a NATURAL RIGHT JOIN r b');
      assert.deepEqual(derived.columns, using.columns);
      assert.deepEqual(derived.rows, using.rows);
      for (const [projection, bindings, expected] of [
        ['id', {}, new Record('docs','saved')],
        ['$flag', {$flag:true}, true],
        ['$bytes', {$bytes:bytes}, bytes],
      ]) {
        const typedDerived = 'SELECT k,(SELECT (SELECT k FROM (SELECT 1 AS n) nums WHERE n=1)) AS correlated FROM (SELECT '+projection+' AS k FROM docs) a JOIN (SELECT '+projection+' AS k FROM docs) b USING(k)';
        assert.deepEqual((await client.execute(typedDerived, bindings)).rows, [[expected,expected]]);
        assert.deepEqual((await client.profileSelect(typedDerived, bindings)).result.rows, [[expected,expected]]);
      }
      assert.deepEqual((await client.profileSelect(typed, {$flag:true,$bytes:bytes})).result.rows, result.rows);
      const native = 'WITH q(x,x) AS NOT MATERIALIZED (SELECT 10,20) SELECT v.* FROM docs d JOIN q v ON 1';
      assert.deepEqual((await client.execute(native)).rows, [[10n,20n]]);
      const named = await client.execute('WITH q(x,x) AS (SELECT 10,20) SELECT v.X FROM docs d JOIN q v ON 1');
      assert.deepEqual(named.columns, ['x']);
      assert.deepEqual(named.rows, [[10n]]);
      await client.execute('CREATE TABLE duplicate_copy(a,b CHECK(b<>a))');
      await client.execute('WITH q(x,x) AS (SELECT 10,20) INSERT INTO duplicate_copy SELECT v.* FROM docs d JOIN q v ON 1');
      assert.deepEqual(await client.all('SELECT * FROM duplicate_copy'), [[10n,20n]]);
      const nestedWrite = await client.execute('WITH docs AS (SELECT 2 AS value), chosen AS (WITH docs AS (SELECT value FROM main.docs), local_q("Key") AS MATERIALIZED (SELECT value FROM docs) SELECT q."KEY" AS value FROM local_q q) UPDATE docs SET value=$next WHERE value IN (SELECT value FROM chosen) RETURNING value', {$next:8n});
      assert.deepEqual(nestedWrite.rows, [[8n]]);
      assert.equal(nestedWrite.affected, 1n);
      assert.equal(nestedWrite.transaction.after, 'active');
      const nestedAudit = await client.checkCollectionIntegrity('docs');
      assert.equal(nestedAudit.documents, 1n);
      assert.equal(nestedAudit.indexEntries, 1n);
      for (const sql of [
        'CREATE TABLE package_keys', "INSERT INTO package_keys(k) VALUES(1),(2),('a')",
        'CREATE TABLE package_rhs(k INTEGER)', "INSERT INTO package_rhs VALUES(1),(3),('a')",
        'CREATE TABLE package_nums(n INTEGER)', 'INSERT INTO package_nums VALUES(0),(1),(2),(NULL)',
        'CREATE TABLE package_sink', 'CREATE UNIQUE INDEX package_sink_k ON package_sink(k)',
        'INSERT INTO package_sink(k) VALUES(3)',
      ]) await client.execute(sql);
      const scalarQuery = 'WITH q(v) AS MATERIALIZED(SELECT $needle COLLATE NOCASE) SELECT k,(SELECT max(b.n) FROM package_nums b WHERE b.n<k AND (SELECT v FROM q)=k) AS v FROM package_keys a RIGHT JOIN package_rhs b USING(k) ORDER BY k';
      const scalarRows = [[1n,null],[3n,null],['a',2n]];
      assert.deepEqual((await client.execute(scalarQuery, {$needle:'A'})).rows, scalarRows);
      assert.deepEqual((await client.profileSelect(scalarQuery, {$needle:'A'})).result.rows, scalarRows);
      const memberQuery = 'SELECT k,(SELECT sum(CASE WHEN b.n IN(SELECT k) THEN 1 WHEN b.n NOT IN(SELECT k) THEN 10 ELSE 100 END) FROM package_nums b WHERE k IS k) AS v FROM package_keys a RIGHT JOIN package_rhs b USING(k) ORDER BY k';
      assert.deepEqual((await client.execute(memberQuery)).rows, [[1n,121n],[3n,130n],['a',130n]]);
      await client.execute('CREATE TABLE package_native(k)');
      await client.execute("INSERT INTO package_native VALUES(1),(2),('a')");
      const localCteQuery = (source, materialization) => 'SELECT k,(WITH first(m) AS (SELECT n+$shift FROM package_nums),chosen(x) AS ' + materialization + ' (SELECT m FROM first) SELECT max(x) FROM chosen b WHERE x<k) AS prior FROM ' + source + ' a RIGHT JOIN package_rhs b USING(k) ORDER BY k';
      for (const materialization of ['', 'MATERIALIZED', 'NOT MATERIALIZED']) {
        const expected = await client.execute(localCteQuery('package_native',materialization),{$shift:0n});
        const actual = await client.execute(localCteQuery('package_keys',materialization),{$shift:0n});
        assert.deepEqual(actual.columns,expected.columns);
        assert.deepEqual(actual.rows,expected.rows);
        assert.deepEqual((await client.profileSelect(localCteQuery('package_keys',materialization),{$shift:0n})).result.rows,expected.rows);
      }
      await client.execute('CREATE TABLE package_cte_copy');
      await client.execute('CREATE UNIQUE INDEX package_cte_k ON package_cte_copy(k)');
      await client.execute('INSERT INTO package_cte_copy(k) VALUES(3),(9)');
      const localCteInsert = 'INSERT INTO package_cte_copy(k,prior) ' + localCteQuery('package_keys','MATERIALIZED') + ' RETURNING k,prior';
      await assert.rejects(async () => client.execute(localCteInsert),error => error.code === 'FDB_PARAMETER' && error.transaction.after === 'active');
      await assert.rejects(async () => client.execute(localCteInsert,{$shift:0n}),error => error.code === 'FDB_CONSTRAINT' && error.transaction.after === 'active');
      assert.deepEqual(await client.all('SELECT k FROM package_cte_copy ORDER BY k'),[[3n],[9n]]);
      await client.execute('DELETE FROM package_cte_copy WHERE k=3');
      const localCteRetry = await client.execute(localCteInsert,{$shift:0n});
      assert.equal(localCteRetry.affected,3n);
      assert.deepEqual(localCteRetry.rows,(await client.execute(localCteQuery('package_native','MATERIALIZED'),{$shift:0n})).rows);
      const localCteAudit = await client.checkCollectionIntegrity('package_cte_copy');
      assert.equal(localCteAudit.documents,4n);
      assert.equal(localCteAudit.indexEntries,4n);
      const orderedCteQuery = 'SELECT k,(WITH chosen(x) AS MATERIALIZED (SELECT n FROM package_nums) SELECT $value FROM chosen b WHERE x<k ORDER BY x LIMIT $take OFFSET $skip) AS value FROM package_keys a RIGHT JOIN package_rhs b USING(k) ORDER BY k';
      for (const value of [true,new Record('package_keys',7n),Buffer.from([0,255])]) {
        const params = {$value:value,$take:'1.0',$skip:'1e0'};
        const expected = [[1n,null],[3n,value],['a',value]];
        assert.deepEqual((await client.execute(orderedCteQuery,params)).rows,expected);
        assert.deepEqual((await client.profileSelect(orderedCteQuery,params)).result.rows,expected);
      }
      assert.deepEqual((await client.execute(orderedCteQuery,{$value:true,$take:'.',$skip:'0'})).rows,[[1n,null],[3n,null],['a',null]]);
      await client.execute('CREATE TABLE package_page_copy');
      await client.execute('CREATE UNIQUE INDEX package_page_k ON package_page_copy(k)');
      await client.execute('INSERT INTO package_page_copy(k) VALUES(3),(9)');
      const orderedCteInsert = 'INSERT INTO package_page_copy(k,value) ' + orderedCteQuery + ' RETURNING k,value';
      const pageParams = {$value:new Record('package_keys',7n),$take:'1.0',$skip:'1e0'};
      await assert.rejects(async () => client.execute(orderedCteInsert,{$value:true,$take:'1.0'}),error => error.code === 'FDB_PARAMETER' && error.transaction.after === 'active');
      await assert.rejects(async () => client.execute(orderedCteInsert,pageParams),error => error.code === 'FDB_CONSTRAINT' && error.transaction.after === 'active');
      assert.deepEqual(await client.all('SELECT k FROM package_page_copy ORDER BY k'),[[3n],[9n]]);
      await client.execute('DELETE FROM package_page_copy WHERE k=3');
      const pageRetry = await client.execute(orderedCteInsert,pageParams);
      assert.equal(pageRetry.affected,3n);
      assert.deepEqual(pageRetry.rows,[[1n,null],[3n,pageParams.$value],['a',pageParams.$value]]);
      const pageAudit = await client.checkCollectionIntegrity('package_page_copy');
      assert.equal(pageAudit.documents,4n);
      assert.equal(pageAudit.indexEntries,4n);
      const compoundQuery = (source, operator, direction) => {
        const rhs = 'SELECT k+1 ' + operator + ' SELECT NULL ORDER BY "k+1"' + direction + ' LIMIT $take OFFSET $skip';
        return 'SELECT k,(SELECT sum(CASE WHEN n IN(' + rhs + ') THEN 1 WHEN n NOT IN(' + rhs + ') THEN 10 ELSE 100 END) FROM package_nums x WHERE k IS k) AS v FROM ' + source + ' a RIGHT JOIN package_rhs b USING(k) ORDER BY k';
      };
      const compoundParams = {$take:1n,$skip:0n};
      for (const operator of ['UNION ALL','UNION','INTERSECT','EXCEPT']) {
        for (const direction of ['', ' DESC']) {
          const expected = await client.execute(compoundQuery('package_native',operator,direction),compoundParams);
          const actual = await client.execute(compoundQuery('package_keys',operator,direction),compoundParams);
          assert.deepEqual(actual.columns,expected.columns);
          assert.deepEqual(actual.rows,expected.rows);
          assert.deepEqual((await client.profileSelect(compoundQuery('package_keys',operator,direction),compoundParams)).result.rows,expected.rows);
        }
      }
      await client.execute('CREATE TABLE package_compound_sink');
      await client.execute('CREATE UNIQUE INDEX package_compound_k ON package_compound_sink(k)');
      await client.execute('INSERT INTO package_compound_sink(k) VALUES(3),(9)');
      const compoundInsert = 'INSERT INTO package_compound_sink(k,v) ' + compoundQuery('package_keys','UNION',' DESC') + ' RETURNING k,v';
      await assert.rejects(async () => client.execute(compoundInsert,{$take:1n}), error => error.code === 'FDB_PARAMETER' && error.transaction.after === 'active');
      await assert.rejects(async () => client.execute(compoundInsert,compoundParams), error => error.code === 'FDB_CONSTRAINT' && error.transaction.after === 'active');
      assert.deepEqual(await client.all('SELECT k FROM package_compound_sink ORDER BY k'),[[3n],[9n]]);
      await client.execute('DELETE FROM package_compound_sink WHERE k=3');
      const compoundRetry = await client.execute(compoundInsert,compoundParams);
      assert.equal(compoundRetry.affected,3n);
      assert.deepEqual(compoundRetry.rows,await client.all(compoundQuery('package_native','UNION',' DESC'),compoundParams));
      const compoundAudit = await client.checkCollectionIntegrity('package_compound_sink');
      assert.equal(compoundAudit.documents,4n);
      assert.equal(compoundAudit.indexEntries,4n);
      await client.execute('CREATE TABLE package_typed');
      const typedQuery = 'SELECT d.n,(SELECT $same FROM package_nums x WHERE x.n=0 AND d.k IN(SELECT d.k INTERSECT SELECT $same LIMIT 1)) AS value FROM package_typed d ORDER BY d.n';
      for (const value of [true,1n,new Record('package_typed',7n),Buffer.from([0,255])]) {
        await client.execute('INSERT INTO package_typed(n,k) VALUES(1,$same)',{$same:value});
        assert.deepEqual((await client.execute(typedQuery,{$same:value})).rows,[[1n,value]]);
        assert.deepEqual((await client.profileSelect(typedQuery,{$same:value})).result.rows,[[1n,value]]);
        await client.execute('DELETE FROM package_typed');
      }
      const typedRecord = new Record('package_typed',7n);
      await client.execute('INSERT INTO package_typed(n,k) VALUES(1,$same),(2,$same)',{$same:typedRecord});
      await client.execute('CREATE TABLE package_typed_copy');
      await client.execute('CREATE UNIQUE INDEX package_typed_n ON package_typed_copy(n)');
      await client.execute('INSERT INTO package_typed_copy(n) VALUES(2),(9)');
      const typedInsert = 'INSERT INTO package_typed_copy(n,value) ' + typedQuery + ' RETURNING n,value';
      await assert.rejects(async () => client.execute(typedInsert,{$same:typedRecord}),error => error.code === 'FDB_CONSTRAINT' && error.transaction.after === 'active');
      assert.deepEqual(await client.all('SELECT n FROM package_typed_copy ORDER BY n'),[[2n],[9n]]);
      await client.execute('DELETE FROM package_typed_copy WHERE n=2');
      const typedRetry = await client.execute(typedInsert,{$same:typedRecord});
      assert.equal(typedRetry.affected,2n);
      assert.deepEqual(typedRetry.rows,[[1n,typedRecord],[2n,typedRecord]]);
      const typedAudit = await client.checkCollectionIntegrity('package_typed_copy');
      assert.equal(typedAudit.documents,3n);
      assert.equal(typedAudit.indexEntries,3n);
      const insertQuery = 'INSERT INTO package_sink(k,v) ' + scalarQuery + ' RETURNING k,v';
      await assert.rejects(async () => client.execute(insertQuery, {$needle:'A'}), error => error.code === 'FDB_CONSTRAINT' && error.transaction.after === 'active');
      assert.deepEqual(await client.all('SELECT k FROM package_sink'), [[3n]]);
      await client.execute('DELETE FROM package_sink WHERE k=3');
      const scalarInsert = await client.execute(insertQuery, {$needle:'A'});
      assert.equal(scalarInsert.affected, 3n);
      assert.deepEqual(scalarInsert.rows, scalarRows);
      const scalarAudit = await client.checkCollectionIntegrity('package_sink');
      assert.equal(scalarAudit.documents, 3n);
      assert.equal(scalarAudit.indexEntries, 3n);

    } finally { await client.execute('ROLLBACK'); }
  }
  async function withCompositeCounts(client) {
    await client.execute('BEGIN');
    try {
      await client.execute('CREATE TABLE count_values');
      for (const [n,value] of [[1n,[]],[2n,{a:1n}],[3n,null],[4n,Buffer.alloc(0)]]) {
        await client.execute('INSERT INTO count_values(n,value) VALUES($n,$value)',{$n:n,$value:value});
      }
      const sql='SELECT count(value),count(missing),count(*) FROM count_values';
      assert.deepEqual(await client.exactlyOne(sql),[3n,0n,4n]);
      assert.deepEqual((await client.profileSelect(sql)).result.rows,[[3n,0n,4n]]);
      assert.deepEqual(await client.all('SELECT count(value) OVER (ORDER BY n) FROM count_values ORDER BY n'),[[1n],[2n],[2n],[3n]]);
      assert.deepEqual(await client.exactlyOne('SELECT count($value) FROM count_values',{$value:{nested:[]}}),[4n]);
      assert.deepEqual(await client.exactlyOne('SELECT count(array::append(value,1)) FILTER (WHERE n=1) FROM count_values'),[1n]);
    } finally { await client.execute('ROLLBACK'); }
  }
  async function withWrites(client) {
    await client.execute('BEGIN');
    await assert.rejects(async()=>client.exactlyOne('SELECT value FROM docs WHERE 0'), error=>isFastDBError(error) && error instanceof RangeError && error.code==='FDB_CARDINALITY' && error.transaction.before==='active' && error.transaction.after==='active');
    const changed = await client.execute('WITH chosen AS (SELECT value FROM docs) UPDATE docs SET value=(SELECT $next) WHERE value IN (SELECT value FROM chosen) RETURNING value', {$next:8n});
    assert.deepEqual(changed.rows, [[8n]]);
    assert.equal(changed.affected,1n);
    assert.equal((await client.profileSelect('SELECT record::fetch($id)', {$id:new Record('docs','saved')})).result.rows[0][0].value,8n);
    const removed = await client.execute('WITH chosen AS (SELECT $value AS value) DELETE FROM docs WHERE value IN (SELECT value FROM chosen) RETURNING value', {$value:8n});
    assert.deepEqual(removed.rows,[[8n]]);
    assert.equal((await client.checkCollectionIntegrity('docs')).documents,0n);
    await client.execute('ROLLBACK');
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0],9223372036854775807n);
    await client.execute('BEGIN');
    const nested=await client.execute('UPDATE docs SET value=(SELECT 1) IN (SELECT 1) RETURNING value');
    assert.deepEqual(nested.rows,[[1n]]);
    assert.equal(nested.affected,1n);
    assert.equal((await client.checkCollectionIntegrity('docs')).indexEntries,1n);
    await client.execute('ROLLBACK');
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0],9223372036854775807n);

    await client.execute('BEGIN');
    const correlated=await client.execute('UPDATE docs AS d SET value=(SELECT $next WHERE d.value>$next) RETURNING value',{$next:8n});
    assert.deepEqual(correlated.rows,[[8n]]);
    assert.equal(correlated.affected,1n);
    assert.deepEqual((await client.profileSelect('SELECT (SELECT $n WHERE d.value=$n) FROM docs AS d',{$n:8n})).result.rows,[[8n]]);
    assert.deepEqual(await client.all('SELECT (SELECT 1 WHERE d.value<0) FROM docs AS d'),[[null]]);
    const correlatedDelete=await client.execute('DELETE FROM docs AS d WHERE EXISTS(SELECT 1 WHERE d.value=$n) RETURNING value',{$n:8n});
    assert.deepEqual(correlatedDelete.rows,[[8n]]);
    assert.equal((await client.checkCollectionIntegrity('docs')).indexEntries,0n);
    await client.execute('ROLLBACK');
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0],9223372036854775807n);

    await client.execute('BEGIN');
    const memberUpdate=await client.execute('UPDATE docs AS d SET value=$next WHERE d.value IN(SELECT $current WHERE d.value>0) RETURNING value',{$next:8n,$current:9223372036854775807n});
    assert.deepEqual(memberUpdate.rows,[[8n]]);
    assert.equal(memberUpdate.affected,1n);
    const membership='SELECT d.value IN(SELECT $n WHERE d.value>0),d.value NOT IN(SELECT $n WHERE d.value<0),d.value IN(SELECT NULL WHERE d.value>0) FROM docs AS d';
    assert.deepEqual((await client.profileSelect(membership,{$n:8n})).result.rows,[[1n,1n,null]]);
    assert.equal((await client.checkCollectionIntegrity('docs')).indexEntries,1n);
    await client.execute('ROLLBACK');
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0],9223372036854775807n);

    await client.execute('BEGIN');
    for (const meta of [{deep:{n:8n}},null,7n,[],{}]) {
      await client.execute('UPDATE docs SET meta=$meta',{$meta:meta});
      const expected=meta && !Array.isArray(meta) && typeof meta==='object' && meta.deep ? 8n : null;
      const derived='SELECT d.meta.deep.n FROM (SELECT meta FROM docs) AS d';
      assert.deepEqual(await client.all(derived),[[expected]]);
      const nested='SELECT (SELECT $n WHERE d.meta.deep.n=$n) FROM (SELECT meta FROM docs) AS d';
      assert.deepEqual((await client.profileSelect(nested,{$n:8n})).result.rows,[[expected]]);
    }
    assert.equal((await client.checkCollectionIntegrity('docs')).indexEntries,1n);
    await client.execute('ROLLBACK');
    assert.deepEqual(await client.all('SELECT meta FROM docs'),[[null]]);
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0],9223372036854775807n);

    await client.execute('BEGIN');
    const projected = await client.execute('UPDATE docs AS d SET value=(SELECT d.value-$delta) RETURNING value',{$delta:9223372036854775799n});
    assert.deepEqual(projected.rows,[[8n]]);
    assert.equal(projected.affected,1n);
    const projectedRead = await client.profileSelect("SELECT (SELECT d.id),(SELECT d.value),d.id IN(SELECT d.id),(SELECT CAST(d.value AS TEXT))=8 FROM docs d");
    assert.deepEqual(projectedRead.result.rows,[[new Record('docs','saved'),8n,1n,1n]]);
    assert.deepEqual(await client.all('SELECT (SELECT d.id WHERE 0) FROM docs d'),[[null]]);
    assert.equal((await client.checkCollectionIntegrity('docs')).indexEntries,1n);
    await client.execute('ROLLBACK');
    assert.equal((await client.exactlyOne('SELECT value FROM docs'))[0],9223372036854775807n);

    await client.execute('BEGIN');
    try {
      await client.execute('CREATE TABLE sort_inputs(n)');
      await client.execute('INSERT INTO sort_inputs VALUES(1),(1.0),(2),(10)');
      const sorted = await client.execute('UPDATE docs AS d SET value=(SELECT CASE WHEN d.value>0 THEN n ELSE d.value END AS x FROM sort_inputs ORDER BY x+0 DESC LIMIT 1) RETURNING value');
      assert.deepEqual(sorted.rows,[[10n]]);
      assert.equal(sorted.affected,1n);
      const distinct = 'SELECT (SELECT DISTINCT CASE WHEN d.value>0 THEN n ELSE d.value END AS x FROM sort_inputs ORDER BY x,n LIMIT $limit OFFSET $offset) FROM docs d';
      assert.deepEqual((await client.profileSelect(distinct,{$limit:1n,$offset:1n})).result.rows,[[2n]]);
      assert.deepEqual(await client.all(distinct,{$limit:0n,$offset:0n}),[[null]]);
      const empty = 'SELECT (SELECT DISTINCT d.id AS x FROM sort_inputs ORDER BY x,n LIMIT 1 OFFSET 1),d.id IN(SELECT DISTINCT d.id AS x FROM sort_inputs ORDER BY x,n LIMIT 1 OFFSET 1) FROM docs d';
      assert.deepEqual((await client.profileSelect(empty)).result.rows,[[null,0n]]);
      assert.equal((await client.checkCollectionIntegrity('docs')).indexEntries,1n);
    } finally {
      await client.execute('ROLLBACK');
    }
    assert.deepEqual(await client.all('SELECT value FROM docs WHERE value=9223372036854775807'),[[9223372036854775807n]]);

    const cte='WITH docs AS (SELECT 2 AS n) SELECT d.n FROM docs AS d';
    assert.deepEqual(await client.all(cte),[[2n]]);
  }
  const bufferOptions = {writeBufferLimits:{maxRows:1n,maxPayloadBytes:1000n}};
  for (const client of [new Database(':memory:',bufferOptions),await AsyncDatabase.open(':memory:',bufferOptions)]) {
    try {
      await client.execute('CREATE TABLE buffered');
      await client.execute('CREATE UNIQUE INDEX buffered_n ON buffered(n)');
      await client.execute('INSERT INTO buffered {id:buffered:a,n:1}');
      await client.execute('BEGIN');
      await client.execute('INSERT INTO buffered {id:buffered:b,n:2}');
      await assert.rejects(async () => client.execute('UPDATE buffered SET n=n+10'), error => {
        assert.equal(error.code,'FDB_LIMIT');
        assert.deepEqual(error.transaction,{before:'active',after:'active'});
        return true;
      });
      assert.deepEqual(await client.all('SELECT n FROM buffered ORDER BY n'),[[1n],[2n]]);
      await client.execute('UPDATE buffered SET n=3 WHERE n=2');
      assert.deepEqual(await client.all('SELECT n FROM buffered ORDER BY n'),[[1n],[3n]]);
      await client.execute('ROLLBACK');
      assert.deepEqual(await client.all('SELECT n FROM buffered'),[[1n]]);
      assert.equal((await client.checkCollectionIntegrity('buffered')).indexEntries,1n);
    } finally { await client.close(); }
  }
  const file = path.join(__dirname, 'database.db');
  const db = new Database(file);
  try {
    db.execute('CREATE TABLE docs');
    db.execute('CREATE UNIQUE INDEX docs_value ON docs(value)');
    db.execute('INSERT INTO docs (id,value) VALUES ($id,$value)', { $id: new Record('docs','saved'), $value: 9223372036854775807n });
    const audit = db.checkCollectionIntegrity('docs', {maxDocuments: 1n});
    assert.equal(audit.documents, 1n);
    assert.equal(audit.indexEntries, 1n);
    assert.ok(audit.encodedBytes > 0n);
    assert.equal(db.checkCollectionIntegrity('docs', {maxEncodedBytes: audit.encodedBytes}).encodedBytes, audit.encodedBytes);
    const profile = db.profileSelect('SELECT id,value FROM docs WHERE value=$value', {$value: 9223372036854775807n});
    assert(profile.result.rows[0][0] instanceof Record);
    assert.equal(profile.result.rows[0][1], 9223372036854775807n);
    assert.ok(profile.metrics.vmSteps > 0n);
    assert.ok(profile.metrics.btreeSeeks > 0n);
    assert.equal(typeof profile.metrics.indexSteps, 'bigint');
    assert.equal(profile.metrics.rowsWritten, 0n);
    const fetched = db.profileSelect('SELECT record::fetch($id) AS a,record::fetch($id) AS b', {$id:new Record('docs','saved')});
    assert.equal(fetched.metrics.fetchBatches,1n);
    assert(fetched.metrics.fetchRowsRead>0n);
    assert(fetched.metrics.fetchVmSteps>0n);
    assert.equal(fetched.result.rows[0][0].value,9223372036854775807n);
    assert.deepEqual(fetched.result.rows[0][0],fetched.result.rows[0][1]);
    assert.equal(profile.metrics.fetchBatches,0n);

    for (const make of [Vector.float32, Vector.float64, Vector.sparse32, Vector.quantized8, Vector.bit1, () => Vector.sparse32Entries(3, [[0,1],[2,-1]])]) {
      const vector = make([1,0,-1]);
      assert.deepEqual(db.exactlyOne('SELECT $v AS v', {$v: vector})[0], vector);
    }
    await withVectorFields(db);
    await withCompositeCounts(db);
    await withDuplicateColumns(db);
    await withWrites(db);
  } finally { db.close(); }
  assert.throws(()=>db.all('SELECT 1'), error=>isFastDBError(error) && error.code==='FDB_CLOSED' && !Object.hasOwn(error,'transaction'));
  const worker = await AsyncDatabase.open(file);
  try {
    for (const make of [Vector.float32, Vector.float64, Vector.sparse32, Vector.quantized8, Vector.bit1, () => Vector.sparse32Entries(3, [[0,1],[2,-1]])]) {
      const vector = make(new Float32Array([1,0,-1]));
      assert.deepEqual((await worker.exactlyOne('SELECT $v AS v', {$v: vector}))[0], vector);
    }
    await withVectorFields(worker);
    const row = await worker.exactlyOne('SELECT id,value FROM docs');
    assert(row[0] instanceof Record);
    assert.equal(row[0].key, 'saved');
    assert.equal(row[1], 9223372036854775807n);
    await worker.execute('BEGIN');
    await worker.execute('UPDATE docs SET value=7');
    await assert.rejects(worker.execute('DELETE FROM docs',{}, {timeoutMs:0}), error => {
      assert.equal(error.code,'FDB_CANCELLED');
      assert.deepEqual(error.transaction,{before:'active',after:'active'});
      return true;
    });
    assert.deepEqual(await worker.all('SELECT value FROM docs',{}, {timeoutMs:60000}),[[7n]]);
    await assert.rejects(worker.execute('DELETE FROM docs',{}, {timeoutMs:-1}), /timeoutMs/);

    const cancelled = new AbortController(); cancelled.abort();
    const options = {signal:cancelled.signal};
    for (const operation of [
      () => worker.execute('DELETE FROM docs', {}, options),
      () => worker.all('SELECT * FROM docs', {}, options),
      () => worker.first('SELECT * FROM docs', {}, options),
      () => worker.exactlyOne('SELECT * FROM docs', {}, options),
      () => worker.executeBatch('DELETE FROM docs;', options),
      () => worker.profileSelect('SELECT * FROM docs', {}, options),
      () => worker.checkCollectionIntegrity('docs', {}, options),
      () => worker.exportDocuments('docs', 'json', options),
      () => worker.importDocuments('docs', 'invalid', 'ndjson', options),
      () => worker.migrate([], options),
    ]) {
      await assert.rejects(operation(), error => error.code === 'FDB_CANCELLED' && error.transaction.after === 'active');
    }
    assert.equal(require('node:events').getEventListeners(cancelled.signal,'abort').length,0);
    const importSource=new Database();
    let payload;
    try {
      importSource.execute('CREATE TABLE docs');
      importSource.execute('INSERT INTO docs(value) VALUES '+Array.from({length:1000},(_,i)=>'('+(10000+i)+')').join(','));
      payload=importSource.exportDocuments('docs','json');
    } finally {importSource.close();}
    await worker.execute('SAVEPOINT import_check');
    const importing=new AbortController();
    const pendingImport=worker.importDocuments('docs',payload,'json',{signal:importing.signal});
    const timer=setTimeout(()=>importing.abort(),20);
    try {
      await assert.rejects(pendingImport,error=>error.code==='FDB_CANCELLED' && error.transaction.after==='active');
    } finally {clearTimeout(timer);}
    assert.equal((await worker.checkCollectionIntegrity('docs')).documents,1n);
    assert.deepEqual(await worker.exactlyOne('SELECT value FROM docs'),[7n]);
    assert.equal(require('node:events').getEventListeners(importing.signal,'abort').length,0);
    assert.equal((await worker.importDocuments('docs',payload,'json')).imported,1000);
    assert.equal((await worker.checkCollectionIntegrity('docs')).documents,1001n);
    await worker.execute('ROLLBACK TO import_check');
    await worker.execute('RELEASE import_check');
    assert.deepEqual(await worker.exactlyOne('SELECT value FROM docs'),[7n]);
    const fresh = new AbortController();
    const result = await worker.executeBatch('SELECT value FROM docs;', {signal:fresh.signal});
    assert.deepEqual(result[0].result.rows, [[7n]]);
    fresh.abort();
    assert.deepEqual(await worker.exactlyOne('SELECT value FROM docs'), [7n]);

    await assert.rejects(worker.checkCollectionIntegrity('docs', {maxDocuments: 0n}), error => error.code === 'FDB_LIMIT' && error.transaction.after === 'active');
    const profile = await worker.profileSelect('SELECT value FROM docs WHERE value=$value', {$value: 7n});
    assert.equal(profile.result.rows[0][0], 7n);
    assert.equal(profile.result.transaction.after, 'active');
    assert.ok(profile.metrics.btreeSeeks > 0n);
    assert.equal(typeof profile.metrics.indexSteps, 'bigint');
    assert.deepEqual((await worker.profileSelect('SELECT value FROM docs WHERE value=$value', {$value: 7n})).metrics, profile.metrics);

    const fetched = await worker.profileSelect('SELECT record::fetch($id)', {$id:new Record('docs','saved')});
    assert.equal(fetched.result.rows[0][0].value,7n);
    assert.equal(fetched.result.transaction.after,'active');
    assert.equal(fetched.metrics.fetchBatches,1n);
    assert(fetched.metrics.fetchRowsRead>0n);
    assert(fetched.metrics.fetchVmSteps>0n);
    assert.equal(profile.metrics.fetchBatches,0n);
    await worker.execute('ROLLBACK');
    assert.equal((await worker.exactlyOne('SELECT value FROM docs'))[0], 9223372036854775807n);
    assert.equal((await worker.checkCollectionIntegrity('docs')).documents, 1n);
    await withCompositeCounts(worker);
    await withDuplicateColumns(worker);
    await withWrites(worker);
  } finally { await worker.close(); }
  await assert.rejects(worker.all('SELECT 1'), error=>isFastDBError(error) && error.code==='FDB_CLOSED' && !Object.hasOwn(error,'transaction'));
  const reopened = new Database(file);
  try {
    assert.equal(reopened.exactlyOne('SELECT value FROM docs')[0], 9223372036854775807n);
    assert.equal(reopened.checkCollectionIntegrity('docs').indexEntries, 1n);
  }
  finally { reopened.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
`);
  run(process.execPath, ['smoke.cjs'], consumer);
  // Exercise missing/incompatible artifacts only inside the temporary install.
  const installedAddon = path.join(consumer, 'node_modules/@fastdb/node/fastdb.node');
  const savedAddon = installedAddon + '.saved';
  fs.renameSync(installedAddon, savedAddon);
  try {
    const failureProbe = `const assert = require('node:assert/strict');
      assert.throws(()=>require('@fastdb/node'), error => {
        assert.equal(error.code, 'FDB_NATIVE_LOAD');
        assert(error.cause instanceof Error);
        assert(error.message.includes(process.platform + '/' + process.arch));
        assert(error.message.includes('check-node.sh'));
        return true;
      });`;
    run(process.execPath, ['-e', failureProbe], consumer);
    fs.writeFileSync(installedAddon, 'invalid native addon');
    run(process.execPath, ['-e', failureProbe], consumer);
  } finally {
    fs.rmSync(installedAddon, {force:true});
    fs.renameSync(savedAddon, installedAddon);
  }

  // Check declaration resolution from the installed package, with the local
  // compiler as a tool only; the package has no runtime registry dependencies.
  fs.writeFileSync(path.join(consumer, 'smoke.ts'), `import { Database, AsyncDatabase, Record, Vector, VectorComponents, SparseVectorEntry, IntegrityLimits, IntegrityReport, ProfiledQuery } from '@fastdb/node';
function inspectError(error: unknown): string | undefined {
  if (guard(error)) {
    const typed: import('@fastdb/node').FastDBError = error;
    return typed.transaction?.after ?? typed.code;
  }
  return undefined;
}
import { isFastDBError as guard } from '@fastdb/node';
void inspectError;
const db = new Database();
db.execute('SELECT $id', { $id: new Record('docs', 1n) });
const components: VectorComponents = [1,0,-1] as const;
const vector: Vector = Vector.quantized8(components);
void vector;
const entries: readonly SparseVectorEntry[] = [[0,1],[2,-1]] as const;
Vector.sparse32Entries(3, entries);
// @ts-expect-error sparse values require numbers
Vector.sparse32Entries(3, [[0,1n]]);
// @ts-expect-error components require numbers
Vector.bit1([1n]);
const limits: IntegrityLimits = {maxDocuments: 1n};
const audit: IntegrityReport = db.checkCollectionIntegrity('docs', limits);
const profile: ProfiledQuery = db.profileSelect('SELECT 1');
const resultBudget: import('@fastdb/node').ResultLimits = {maxRows:1n,maxPayloadBytes:100n};
const connectionOptions: import('@fastdb/node').DatabaseOptions = {writeBufferLimits:resultBudget};
new Database(':memory:',connectionOptions).close();
void AsyncDatabase.open(':memory:',connectionOptions);
// @ts-expect-error buffer budgets require bigint
new Database(':memory:',{writeBufferLimits:{maxRows:1,maxPayloadBytes:100n}});
// @ts-expect-error unknown connection option
AsyncDatabase.open(':memory:',{unknown:true});
db.selectWithLimits('SELECT 1',resultBudget);
db.profileSelectWithLimits('SELECT 1',resultBudget);
db.writeWithResultLimits('DELETE FROM docs',resultBudget);
// @ts-expect-error both result budgets are mandatory
db.selectWithLimits('SELECT 1',{maxRows:1n});
const counts: bigint[] = [audit.documents, audit.encodedBytes, profile.metrics.vmSteps, profile.metrics.fetchBatches, profile.metrics.fetchRowsRead, profile.metrics.fetchVmSteps];
// @ts-expect-error lossless limits require bigint
db.checkCollectionIntegrity('docs', {maxDocuments: 1});
void counts;
db.close();
async function open() {
  const db = await AsyncDatabase.open();
  const options: import('@fastdb/node').ExecuteOptions = {signal:new AbortController().signal,timeoutMs:1000};
  await db.selectWithLimits('SELECT 1',resultBudget,{},options);
  await db.profileSelectWithLimits('SELECT 1',resultBudget,{},options);
  await db.writeWithResultLimits('DELETE FROM docs',resultBudget,{},options);
  await db.execute('SELECT 1', {}, options);
  await db.all('SELECT 1', {}, options);
  await db.first('SELECT 1', {}, options);
  await db.exactlyOne('SELECT 1', {}, options);
  await db.executeBatch('SELECT 1;', options);
  await db.profileSelect('SELECT 1', {}, options);
  await db.checkCollectionIntegrity('docs', {}, options);
  await db.exportDocuments('docs', 'json', options);
  await db.importDocuments('docs', '', 'ndjson', options);
  await db.migrate([], options);
  // @ts-expect-error cancellation requires an AbortSignal
  await db.migrate([], {signal:true});

  const audit: IntegrityReport = await db.checkCollectionIntegrity('docs', limits);
  const profile: ProfiledQuery = await db.profileSelect('SELECT 1');
  const counts: bigint[] = [audit.indexEntries, profile.metrics.rowsRead, profile.metrics.fetchBatches, profile.metrics.fetchRowsRead, profile.metrics.fetchVmSteps];
  void counts;
  await db.close();
}
void open;
`);
  run(process.execPath, [path.join(packageDir, 'node_modules/typescript/bin/tsc'), '--noEmit', '--strict', '--target', 'ES2022', '--module', 'commonjs', 'smoke.ts'], consumer);
  console.log(`Node package smoke passed: ${process.platform}/${process.arch}, Node ${process.versions.node}, ${packed.entryCount} files, ${packed.size} packed bytes`);
} finally {
  fs.rmSync(temporary, { recursive: true, force: true });
}
