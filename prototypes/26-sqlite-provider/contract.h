/* #43 declaration-only design: NO implementation or supported binary ABI.
 * Closed provider-specific C interface. No public Rill FFI is authorized.
 * See docs/sqlite-provider-binding-v1.md for leases, frames and ownership.
 */
#ifndef RILL_SQLITE_PROVIDER_CONTRACT_V1_H
#define RILL_SQLITE_PROVIDER_CONTRACT_V1_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

enum rill_sqlite_v1_status {
  RSQL_OK = 0, RSQL_PENDING = 1, RSQL_READY = 2, RSQL_CAPACITY = 3,
  RSQL_CLOSED = 4, RSQL_STALE = 5, RSQL_FAULT = 6, RSQL_BOOTSTRAP_FAILURE = 7
};
/* Fixed trusted runtime hook, NOT a Rill closure/application callback.
 * context is retained native runtime infrastructure, never a Rill frame.
 */
typedef void (*rill_sqlite_v1_notify)(void *context, uint64_t wake_id,
                                      uint64_t wake_generation);

/* Runtime-only start/stop run on blocking bootstrap/finalization machinery.
 * All byte spans/output pointers are temporary leased access until return.
 * Only context/notify deliberately live until shutdown has joined notifications.
 */
uint32_t rill_sqlite_v1_service_start(
    void *context, rill_sqlite_v1_notify notify,
    const uint8_t *config, uint64_t config_bytes, uint64_t *service,
    uint8_t *error_frame, uint64_t error_capacity, uint64_t *error_bytes);
uint32_t rill_sqlite_v1_submit(
    uint64_t service, const uint8_t *request, uint64_t request_bytes,
    uint64_t wake_id, uint64_t wake_generation, uint8_t job[32]);
uint32_t rill_sqlite_v1_request_stop(uint64_t service, const uint8_t job[32]);
uint32_t rill_sqlite_v1_observe(
    uint64_t service, const uint8_t job[32], uint64_t *result_bytes);
uint32_t rill_sqlite_v1_copy_result(
    uint64_t service, const uint8_t job[32], uint8_t *destination,
    uint64_t capacity, uint64_t *written);
/* Called only inside the registered, intrinsic-delivery-deferred handoff. */
uint32_t rill_sqlite_v1_adopt(
    uint64_t service, const uint8_t job[32], const uint8_t offer[32],
    uint64_t registered_adoption_receipt);
/* Quiescent only; live offers must first be adopted or disposed and joined. */
uint32_t rill_sqlite_v1_release_job(uint64_t service, const uint8_t job[32]);
uint32_t rill_sqlite_v1_service_stop(
    uint64_t service, uint8_t *error_frame, uint64_t error_capacity,
    uint64_t *error_bytes);

#ifdef __cplusplus
}
#endif
#endif
