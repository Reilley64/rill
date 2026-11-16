/* Compiled separately with the SDK header, without private prefix macros. */
#include <sqlite3.h>
#include <stdio.h>
static sqlite3 *host_db;
int host_reference(void) {
  int rc = sqlite3_config(SQLITE_CONFIG_SINGLETHREAD);
  printf("HOST config_SINGLETHREAD_rc=%d version=%s sourceID=%s threadsafe=%d\n", rc, sqlite3_libversion(), sqlite3_sourceid(), sqlite3_threadsafe());
  if (rc != SQLITE_OK)
    printf("UNTESTED host SINGLETHREAD/null-mutex discrimination: system config rejected\n");
  int init = sqlite3_initialize();
  int opened = sqlite3_open(":memory:", &host_db);
  printf("HOST initialize_rc=%d open_rc=%d\n", init, opened);
  if (init != SQLITE_OK || opened != SQLITE_OK) return 1;
  printf("HOST db_mutex=%s (host remains live during private tests)\n",
         sqlite3_db_mutex(host_db) ? "nonnull" : "null");
  if (rc == SQLITE_OK && sqlite3_db_mutex(host_db) != 0) return 2;
  return 0;
}
int host_finish(void) {
  sqlite3_stmt *s = 0;
  int ok = sqlite3_prepare_v2(host_db, "SELECT 41+1", -1, &s, 0) == SQLITE_OK;
  if (ok) ok = sqlite3_step(s) == SQLITE_ROW && sqlite3_column_int(s, 0) == 42;
  if (s && sqlite3_finalize(s) != SQLITE_OK) ok = 0;
  if (sqlite3_close(host_db) != SQLITE_OK) ok = 0;
  if (sqlite3_shutdown() != SQLITE_OK) ok = 0;
  return !ok;
}
