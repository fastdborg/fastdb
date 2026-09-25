# FastDB for PHP

Native embedded FastDB/FastQL for 64-bit PHP with the FFI extension. This is a
local development package; see [qualification and limits](../../docs/native-language-clients.md).

Build `libfastdb_c.so` with `cargo build --locked -p fastdb-c` in the repository
root. Use Composer's classmap autoload or require `src/FastDB.php` directly.
Pass an absolute library path:

```php
require 'src/FastDB.php';
use FastDB\Database;
use FastDB\Value;

$db = new Database('app.db', '/absolute/path/libfastdb_c.so');
try {
    $result = $db->execute('SELECT $name', ['$name' => Value::string('hello')]);
    echo $result['rows'][0][0]['value'];
} finally {
    $db->close();
}
```

`call()` exposes the shared protocol and `interrupt()` requests cooperative
cancellation. `FastDBException` provides `diagnostic` and `transaction`.
Values retain lossless tags in results. Run the local fixture with:

```sh
FASTDB_LIBRARY=/absolute/path/libfastdb_c.so php -d ffi.enable=1 tests/smoke.php
```

The application's PHP deployment must permit FFI; CLI enablement does not
configure PHP-FPM. Never carry an open handle across `fork()`.
