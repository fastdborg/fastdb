#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::Database;

fn main() {
    let mut arguments = std::env::args().skip(1);
    let point = arguments.next().expect("crash point");
    let path = arguments.next().expect("database path");
    let database = Database::open(&path).unwrap();
    let connection = database.connect().unwrap();
    match point.as_str() {
        "bootstrap" => {}
        "implicit-registration" => {
            connection.execute("CREATE item:implicit SET n=1").unwrap();
        }
        "schema-ddl" => {
            connection
                .execute("DEFINE TABLE item SCHEMAFULL; DEFINE FIELD n ON item TYPE int")
                .unwrap();
        }
        "index-ddl" => {
            connection
                .execute(
                    "DEFINE TABLE item SCHEMAFULL; DEFINE FIELD n ON item TYPE int; \
                     DEFINE INDEX by_n ON item FIELDS n; CREATE item:indexed SET n=1",
                )
                .unwrap();
        }
        "write" => {
            connection.execute("CREATE item:written SET n=1").unwrap();
        }
        "commit-publication" => {
            connection
                .execute(
                    "BEGIN; DEFINE TABLE item SCHEMAFULL; DEFINE FIELD n ON item TYPE int; \
                     CREATE item:committed SET n=1; COMMIT",
                )
                .unwrap();
        }
        "rollback" => {
            connection
                .execute("BEGIN; CREATE item:rolled_back SET n=1; CANCEL")
                .unwrap();
        }
        "clean-close" => {
            connection.execute("CREATE item:closed SET n=1").unwrap();
            connection.close().unwrap();
            return;
        }
        "migrate-format2" => {
            // Opening the copied format-1 fixture performs and commits the
            // format-2 migration. Exit without close to exercise recovery at
            // the post-publication process boundary.
        }
        "graph-write" => {
            connection
                .execute(
                    "DEFINE TABLE links TYPE RELATION FROM person TO post ENFORCED; \
                     CREATE person:one CONTENT {}; CREATE post:one CONTENT {}; \
                     RELATE person:one->links->post:one SET weight=1",
                )
                .unwrap();
        }
        "graph-cascade" => {
            connection.execute("DELETE person:one").unwrap();
        }
        _ => panic!("unknown crash point {point}"),
    }
    // Test-only abrupt process boundary: bypass Rust drops and engine close.
    std::process::exit(86);
}
