/* Native reference only: no Rill bridge, resource model, or security boundary. */
#include "sqlite3.h"
#include <stdint.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

extern int host_reference(void);
extern int host_finish(void);
static int checks, failures;
#define CHECK(name, expr) do { int ok = !!(expr); ++checks; failures += !ok; \
  printf("%s %03d %s\n", ok ? "PASS" : "FAIL", checks, name); } while (0)
#define MUST(expr) do { if (!(expr)) { fprintf(stderr, "fixture setup failed line %d: %s\n", __LINE__, #expr); exit(2); } } while (0)
static sqlite3_stmt *prep(sqlite3 *db, const char *sql) {
  sqlite3_stmt *s = 0;
  MUST(sqlite3_prepare_v2(db, sql, -1, &s, 0) == SQLITE_OK);
  return s;
}
static int scalar(sqlite3 *db, const char *sql) {
  sqlite3_stmt *s = prep(db, sql);
  MUST(sqlite3_step(s) == SQLITE_ROW);
  int n = sqlite3_column_int(s, 0);
  MUST(sqlite3_finalize(s) == SQLITE_OK);
  return n;
}
static void exec_ok(sqlite3 *db, const char *sql) {
  MUST(sqlite3_exec(db, sql, 0, 0, 0) == SQLITE_OK);
}
static sqlite3 *open_db(const char *path) {
  sqlite3 *db = 0;
  MUST(sqlite3_open_v2(path, &db, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE |
       SQLITE_OPEN_FULLMUTEX | SQLITE_OPEN_PRIVATECACHE, 0) == SQLITE_OK);
  int on = 0;
  /* Prove compiled trusted-schema default before connection configuration. */
  MUST(sqlite3_db_config(db, SQLITE_DBCONFIG_TRUSTED_SCHEMA, -1, &on) == SQLITE_OK && !on);
  MUST(sqlite3_db_config(db, SQLITE_DBCONFIG_DEFENSIVE, 1, &on) == SQLITE_OK && on);
  MUST(sqlite3_db_config(db, SQLITE_DBCONFIG_TRUSTED_SCHEMA, 0, &on) == SQLITE_OK && !on);
  MUST(sqlite3_busy_timeout(db, 0) == SQLITE_OK);
  exec_ok(db, "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA query_only=OFF");
  return db;
}
/* The trusted flag is a fixture convenience, NOT a proposed product escape API. */
struct policy { int trusted, deny_read, calls, denied; };
static int auth(void *p, int op, const char *a, const char *b, const char *schema, const char *origin) {
  struct policy *x = p;
  (void)schema; (void)origin;
  ++x->calls;
  if (x->trusted) return SQLITE_OK;
  if (op == SQLITE_TRANSACTION || op == SQLITE_SAVEPOINT || op == SQLITE_ATTACH || op == SQLITE_DETACH) goto deny;
  if (op == SQLITE_READ && x->deny_read) goto deny;
  if (op != SQLITE_PRAGMA) return SQLITE_OK;
  if (!a) goto deny;
  const char *objects[] = {"table_list", "table_info", "table_xinfo", "index_list", "index_info", "index_xinfo", "foreign_key_list"};
  const char *noargs[] = {"database_list", "foreign_keys", "query_only", "trusted_schema", "schema_version", "journal_mode", "synchronous", "compile_options"};
  for (unsigned i = 0; i < sizeof(objects)/sizeof(*objects); ++i)
    if (!strcasecmp(a, objects[i])) return SQLITE_OK;
  for (unsigned i = 0; i < sizeof(noargs)/sizeof(*noargs); ++i)
    if (!strcasecmp(a, noargs[i]) && !b) return SQLITE_OK;
deny:
  ++x->denied;
  return SQLITE_DENY;
}
static int prepare_rc(sqlite3 *db, const char *sql) {
  sqlite3_stmt *s = 0;
  int rc = sqlite3_prepare_v2(db, sql, -1, &s, 0);
  if (s) sqlite3_finalize(s);
  return rc;
}
static int progress(void *p) { ++*(int *)p; return 1; }
int main(int argc, char **argv) {
  MUST(argc == 2);
  CHECK("host separate-TU open (singlethread result reported separately)", host_reference() == 0);
  MUST(sqlite3_config(SQLITE_CONFIG_SERIALIZED) == SQLITE_OK);
  MUST(sqlite3_initialize() == SQLITE_OK);
  printf("PRIVATE version=%s sourceID=%s threadsafe=%d\n", sqlite3_libversion(), sqlite3_sourceid(), sqlite3_threadsafe());
  CHECK("pinned native identity", !strcmp(sqlite3_sourceid(), "2026-10-16 14:45:20 555e07ef33f69a0957d24d8e555b702464a131d6f1607f4c3c1e4551e8e6ca76") && sqlite3_libversion_number() == 3054001);
  CHECK("profile threadsafe serialized compiled", sqlite3_threadsafe() == 1);
  CHECK("batch atomic write absent (not defined as zero)", !sqlite3_compileoption_used("ENABLE_BATCH_ATOMIC_WRITE"));
  CHECK("omits and mmap profile", sqlite3_compileoption_used("OMIT_LOAD_EXTENSION") && sqlite3_compileoption_used("OMIT_SHARED_CACHE") && sqlite3_compileoption_used("MAX_MMAP_SIZE=0"));
  const char *extensions[] = {"ENABLE_FTS3", "ENABLE_FTS4", "ENABLE_FTS5", "ENABLE_RTREE", "ENABLE_GEOPOLY", "ENABLE_DBPAGE_VTAB", "ENABLE_DBSTAT_VTAB"};
  int absent = 1;
  for (unsigned i = 0; i < sizeof(extensions)/sizeof(*extensions); ++i) absent &= !sqlite3_compileoption_used(extensions[i]);
  CHECK("optional extension enable macros absent", absent);
  for (int i = 0; sqlite3_compileoption_get(i); ++i) printf("OPTION %s\n", sqlite3_compileoption_get(i));
  sqlite3 *db = open_db(":memory:");
  CHECK("private FULLMUTEX nonnull independent of host", sqlite3_db_mutex(db) != 0);
  CHECK("connection baseline", scalar(db, "PRAGMA foreign_keys") == 1 && scalar(db, "PRAGMA trusted_schema") == 0 && scalar(db, "PRAGMA query_only") == 0 && scalar(db, "PRAGMA synchronous") == 2);
  int defensive = 0;
  MUST(sqlite3_db_config(db, SQLITE_DBCONFIG_DEFENSIVE, -1, &defensive) == SQLITE_OK);
  CHECK("defensive on", defensive == 1);
  CHECK("core JSON retained", scalar(db, "SELECT json_valid('{\"x\":1}')") == 1);
  sqlite3 *bad = 0;
  int rc = sqlite3_open_v2("missing-parent/no.db", &bad, SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX | SQLITE_OPEN_PRIVATECACHE, 0);
  CHECK("failed open returns nonnull cleanup handle", rc == SQLITE_CANTOPEN && bad != 0);
  CHECK("failed open handle closes", bad && sqlite3_close(bad) == SQLITE_OK);

  sqlite3_stmt *s = prep(db, "SELECT ?1,?2,?3,?4,?5,?6,?7");
  char text[] = "copy";
  MUST(sqlite3_bind_text(s, 1, text, 4, SQLITE_TRANSIENT) == SQLITE_OK);
  text[0] = 'X';
  MUST(sqlite3_bind_blob(s, 2, "", 0, SQLITE_TRANSIENT) == SQLITE_OK);
  MUST(sqlite3_bind_null(s, 3) == SQLITE_OK);
  MUST(sqlite3_bind_text(s, 4, "a\0b", 3, SQLITE_TRANSIENT) == SQLITE_OK);
  MUST(sqlite3_bind_int64(s, 5, INT64_MIN) == SQLITE_OK);
  MUST(sqlite3_bind_int64(s, 6, INT64_MAX) == SQLITE_OK);
  MUST(sqlite3_bind_double(s, 7, NAN) == SQLITE_OK);
  MUST(sqlite3_step(s) == SQLITE_ROW);
  CHECK("transient bind copied before caller mutation", !memcmp(sqlite3_column_text(s, 0), "copy", 4));
  CHECK("empty BLOB differs from NULL by initial type", sqlite3_column_type(s, 1) == SQLITE_BLOB && sqlite3_column_bytes(s, 1) == 0 && sqlite3_column_type(s, 2) == SQLITE_NULL);
  CHECK("embedded NUL text length and bytes preserved", sqlite3_column_type(s, 3) == SQLITE_TEXT && sqlite3_column_bytes(s, 3) == 3 && !memcmp(sqlite3_column_text(s, 3), "a\0b", 3));
  CHECK("i64 extrema exact", sqlite3_column_int64(s, 4) == INT64_MIN && sqlite3_column_int64(s, 5) == INT64_MAX);
  CHECK("native NaN becomes NULL (product must reject before bind)", sqlite3_column_type(s, 6) == SQLITE_NULL);
  unsigned char row[4]; memcpy(row, sqlite3_column_text(s, 0), 4);
  MUST(sqlite3_reset(s) == SQLITE_OK);
  MUST(sqlite3_step(s) == SQLITE_ROW);
  CHECK("reset retains bindings", !memcmp(sqlite3_column_text(s, 0), "copy", 4));
  MUST(sqlite3_reset(s) == SQLITE_OK);
  MUST(sqlite3_clear_bindings(s) == SQLITE_OK);
  MUST(sqlite3_step(s) == SQLITE_ROW);
  CHECK("clear removes bindings", sqlite3_column_type(s, 0) == SQLITE_NULL && sqlite3_column_type(s, 4) == SQLITE_NULL);
  MUST(sqlite3_finalize(s) == SQLITE_OK);
  CHECK("explicit row copy survives reset/finalize", !memcmp(row, "copy", 4));
  s = prep(db, "SELECT 17, '123', x'6162'");
  MUST(sqlite3_step(s) == SQLITE_ROW);
  CHECK("initial types captured before coercion", sqlite3_column_type(s, 0) == SQLITE_INTEGER && sqlite3_column_type(s, 1) == SQLITE_TEXT && sqlite3_column_type(s, 2) == SQLITE_BLOB);
  CHECK("native coercion is not checked decoding", sqlite3_column_int(s, 1) == 123 && !strcmp((const char *)sqlite3_column_text(s, 0), "17"));
  MUST(sqlite3_finalize(s) == SQLITE_OK);
  s = prep(db, "SELECT ?5,:same,:same");
  CHECK("sparse ?5 and repeated named parameter", sqlite3_bind_parameter_count(s) == 6 && sqlite3_bind_parameter_index(s, ":same") == 6 && !strcmp(sqlite3_bind_parameter_name(s, 5), "?5"));
  MUST(sqlite3_finalize(s) == SQLITE_OK);
  s = prep(db, "SELECT ?5");
  CHECK("?5 alone counts five including gaps", sqlite3_bind_parameter_count(s) == 5);
  MUST(sqlite3_finalize(s) == SQLITE_OK);

  exec_ok(db, "CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE); INSERT INTO t VALUES(1,'x')");
  MUST(sqlite3_extended_result_codes(db, 1) == SQLITE_OK);
  rc = sqlite3_exec(db, "INSERT INTO t VALUES(2,'x')", 0, 0, 0);
  CHECK("UNIQUE extended code, no structured key identity API", rc == SQLITE_CONSTRAINT_UNIQUE && sqlite3_extended_errcode(db) == SQLITE_CONSTRAINT_UNIQUE);
  char *error = sqlite3_mprintf("%s", sqlite3_errmsg(db)); MUST(error);
  rc = sqlite3_exec(db, "INSERT INTO t VALUES(1,'y')", 0, 0, 0);
  CHECK("PRIMARY KEY extended code", rc == SQLITE_CONSTRAINT_PRIMARYKEY);
  exec_ok(db, "CREATE TABLE r(x); INSERT INTO r(rowid,x) VALUES(1,1)");
  rc = sqlite3_exec(db, "INSERT INTO r(rowid,x) VALUES(1,2)", 0, 0, 0);
  CHECK("ROWID extended code", rc == SQLITE_CONSTRAINT_ROWID);
  exec_ok(db, "SELECT 1");
  CHECK("explicit diagnostic copy survives native error invalidation", strstr(error, "UNIQUE constraint failed") != 0);
  sqlite3_free(error);
  s = prep(db, "INSERT INTO t VALUES(3,'x')");
  MUST(sqlite3_step(s) == SQLITE_CONSTRAINT_UNIQUE);
  CHECK("reset reports prior step error but retains statement", sqlite3_reset(s) == SQLITE_CONSTRAINT_UNIQUE && sqlite3_next_stmt(db, 0) == s);
  MUST(sqlite3_step(s) == SQLITE_CONSTRAINT_UNIQUE);
  CHECK("finalize consumes even when reporting error", sqlite3_finalize(s) == SQLITE_CONSTRAINT_UNIQUE && sqlite3_next_stmt(db, 0) == 0);

  struct policy p = {0};
  MUST(sqlite3_set_authorizer(db, auth, &p) == SQLITE_OK);
  const char *allowed[] = {"PRAGMA TaBlE_InFo(t)", "PRAGMA table_xinfo(t)", "PRAGMA table_list(t)", "PRAGMA index_list(t)", "PRAGMA index_info(sqlite_autoindex_t_1)", "PRAGMA index_xinfo(sqlite_autoindex_t_1)", "PRAGMA foreign_key_list(t)", "PRAGMA database_list", "PRAGMA foreign_keys", "PRAGMA query_only", "PRAGMA trusted_schema", "PRAGMA schema_version", "PRAGMA journal_mode", "PRAGMA synchronous", "PRAGMA compile_options"};
  for (unsigned i = 0; i < sizeof(allowed)/sizeof(*allowed); ++i) CHECK(allowed[i], prepare_rc(db, allowed[i]) == SQLITE_OK);
  const char *denied[] = {"BEGIN", "COMMIT", "ROLLBACK", "SAVEPOINT x", "RELEASE x", "ROLLBACK TO x", "ATTACH ':memory:' AS a", "DETACH a", "PRAGMA foreign_keys=OFF", "PRAGMA QuErY_OnLy(1)", "PRAGMA trusted_schema=ON", "PRAGMA schema_version=4", "PRAGMA journal_mode=WAL", "PRAGMA synchronous=OFF", "PRAGMA compile_options(1)", "PRAGMA database_list(x)", "PRAGMA unknown_43", "PRAGMA cache_size=17"};
  for (unsigned i = 0; i < sizeof(denied)/sizeof(*denied); ++i) CHECK(denied[i], prepare_rc(db, denied[i]) == SQLITE_AUTH);
  CHECK("denied PRAGMAs did not change baseline", scalar(db, "PRAGMA foreign_keys") == 1 && scalar(db, "PRAGMA query_only") == 0 && scalar(db, "PRAGMA trusted_schema") == 0);
  CHECK("table-valued object PRAGMA accepted", scalar(db, "SELECT count(*) FROM pragma_table_info('t')") == 2);
  CHECK("table-valued noarg PRAGMA accepted", scalar(db, "SELECT query_only FROM pragma_query_only") == 0);
  int before = p.denied;
  rc = sqlite3_exec(db, "SELECT * FROM pragma_cache_size", 0, 0, 0);
  CHECK("table-valued disallowed PRAGMA denied at execution", rc != SQLITE_OK && p.denied > before);
  CHECK("unknown table-valued pragma unavailable", prepare_rc(db, "SELECT * FROM pragma_unknown_43") == SQLITE_ERROR);
  s = prep(db, "SELECT u FROM t");
  p.deny_read = 1; before = p.denied;
  MUST(sqlite3_set_authorizer(db, auth, &p) == SQLITE_OK); /* expires cached statement */
  CHECK("cached statement reprepare rechecks policy", sqlite3_step(s) == SQLITE_AUTH && p.denied > before);
  CHECK("reprepare error finalize consumes", sqlite3_finalize(s) == SQLITE_AUTH && sqlite3_next_stmt(db, 0) == 0);
  p.deny_read = 0;
  p.trusted = 1;
  int oldcache = scalar(db, "PRAGMA cache_size");
  s = prep(db, "PRAGMA cache_size=17"); /* deliberately never step */
  CHECK("warning: without policy PRAGMA mutates during prepare", oldcache != 17 && scalar(db, "PRAGMA cache_size") == 17);
  MUST(sqlite3_finalize(s) == SQLITE_OK);
  CHECK("autocommit initially on", sqlite3_get_autocommit(db) == 1);
  exec_ok(db, "BEGIN"); CHECK("BEGIN clears autocommit", !sqlite3_get_autocommit(db));
  exec_ok(db, "ROLLBACK"); CHECK("ROLLBACK restores autocommit", sqlite3_get_autocommit(db));
  exec_ok(db, "BEGIN; COMMIT"); CHECK("COMMIT restores autocommit", sqlite3_get_autocommit(db));
  s = prep(db, "INSERT INTO t VALUES(9,'nine')");
  exec_ok(db, "PRAGMA query_only=ON"); p.trusted = 0;
  CHECK("cached write blocked after internal query_only ON", sqlite3_step(s) == SQLITE_READONLY);
  CHECK("readonly finalize consumes", sqlite3_finalize(s) == SQLITE_READONLY);
  p.trusted = 1; exec_ok(db, "PRAGMA query_only=OFF"); p.trusted = 0;
  CHECK("query_only write did not insert", scalar(db, "SELECT count(*) FROM t WHERE id=9") == 0);

  printf("LIMIT native LENGTH=%d SQL_LENGTH=%d COLUMN=%d VARIABLE_NUMBER=%d EXPR_DEPTH=%d\n", sqlite3_limit(db, SQLITE_LIMIT_LENGTH, -1), sqlite3_limit(db, SQLITE_LIMIT_SQL_LENGTH, -1), sqlite3_limit(db, SQLITE_LIMIT_COLUMN, -1), sqlite3_limit(db, SQLITE_LIMIT_VARIABLE_NUMBER, -1), sqlite3_limit(db, SQLITE_LIMIT_EXPR_DEPTH, -1));
  CHECK("native column cap lower than common 4096", sqlite3_limit(db, SQLITE_LIMIT_COLUMN, -1) == 2000);
  CHECK("native parameter cap 32766 is lower than common 32767", sqlite3_limit(db, SQLITE_LIMIT_VARIABLE_NUMBER, -1) == 32766); /* native is one lower */
  int old = sqlite3_limit(db, SQLITE_LIMIT_VARIABLE_NUMBER, 4);
  CHECK("lower parameter limit rejects sparse ?5", prepare_rc(db, "SELECT ?5") == SQLITE_ERROR);
  sqlite3_limit(db, SQLITE_LIMIT_VARIABLE_NUMBER, old);
  old = sqlite3_limit(db, SQLITE_LIMIT_COLUMN, 2);
  CHECK("lower column limit rejects three", prepare_rc(db, "SELECT 1,2,3") == SQLITE_ERROR);
  sqlite3_limit(db, SQLITE_LIMIT_COLUMN, old);
  old = sqlite3_limit(db, SQLITE_LIMIT_LENGTH, 32);
  s = prep(db, "SELECT ?");
  CHECK("lower payload limit rejects bind before step", sqlite3_bind_text(s, 1, "123456789012345678901234567890123", 33, SQLITE_TRANSIENT) == SQLITE_TOOBIG);
  MUST(sqlite3_finalize(s) == SQLITE_OK); sqlite3_limit(db, SQLITE_LIMIT_LENGTH, old);
  int hits = 0;
  sqlite3_progress_handler(db, 1, progress, &hits);
  s = 0; rc = sqlite3_prepare_v2(db, "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<10000) SELECT sum(x) FROM n", -1, &s, 0);
  if (rc == SQLITE_OK) rc = sqlite3_step(s);
  CHECK("bounded progress abort (may occur in prepare)", rc == SQLITE_INTERRUPT && hits > 0);
  if (s) sqlite3_finalize(s);
  sqlite3_progress_handler(db, 0, 0, 0);
  s = prep(db, "SELECT 1 UNION ALL SELECT 2");
  MUST(sqlite3_step(s) == SQLITE_ROW); sqlite3_interrupt(db);
  CHECK("interrupt active native statement", sqlite3_step(s) == SQLITE_INTERRUPT);
  CHECK("interrupt finalize consumes", sqlite3_finalize(s) == SQLITE_INTERRUPT);
  CHECK("post-interrupt native connection usable", scalar(db, "SELECT 42") == 42);
  MUST(sqlite3_close(db) == SQLITE_OK);

  sqlite3 *a = open_db(argv[1]), *b = open_db(argv[1]);
  struct policy pa = {.trusted = 1}, pb = {.trusted = 1};
  /* Lock/ownership SQL is internal fixture work; never remove the callback. */
  MUST(sqlite3_set_authorizer(a, auth, &pa) == SQLITE_OK);
  MUST(sqlite3_set_authorizer(b, auth, &pb) == SQLITE_OK);
  CHECK("file connection stays rollback journal (no auto WAL)", scalar(a, "SELECT journal_mode='delete' FROM pragma_journal_mode") == 1 && scalar(a, "PRAGMA synchronous") == 2);
  exec_ok(a, "CREATE TABLE locks(x); BEGIN IMMEDIATE");
  CHECK("busy timeout zero", scalar(b, "PRAGMA busy_timeout") == 0);
  CHECK("contended write returns BUSY without automatic retry", sqlite3_exec(b, "INSERT INTO locks VALUES(1)", 0, 0, 0) == SQLITE_BUSY);
  exec_ok(a, "ROLLBACK");
  exec_ok(b, "INSERT INTO locks VALUES(1)");
  CHECK("explicit later write after unlock succeeds once", scalar(b, "SELECT count(*) FROM locks") == 1);
  MUST(sqlite3_close(a) == SQLITE_OK);
  s = prep(b, "SELECT x FROM locks");
  CHECK("close BUSY leaves connection live", sqlite3_close(b) == SQLITE_BUSY && sqlite3_step(s) == SQLITE_ROW);
  MUST(sqlite3_finalize(s) == SQLITE_OK);
  CHECK("close succeeds after statement released", sqlite3_close(b) == SQLITE_OK);
  b = open_db(":memory:");
  MUST(sqlite3_set_authorizer(b, auth, &pb) == SQLITE_OK);
  s = prep(b, "SELECT 1 UNION ALL SELECT 2");
  MUST(sqlite3_step(s) == SQLITE_ROW);
  CHECK("close_v2 accepts with live statement (zombie)", sqlite3_close_v2(b) == SQLITE_OK);
  /* Do not use the zombie db handle. Its statement retains it until finalize. */
  CHECK("zombie statement can finish", sqlite3_step(s) == SQLITE_ROW && sqlite3_column_int(s, 0) == 2);
  CHECK("zombie final statement finalize releases deferred connection", sqlite3_finalize(s) == SQLITE_OK);
  CHECK("private shutdown", sqlite3_shutdown() == SQLITE_OK);
  CHECK("host connection works after private shutdown", host_finish() == 0);
  printf("SUMMARY checks=%d failures=%d\n", checks, failures);
  return failures ? 1 : 0;
}
