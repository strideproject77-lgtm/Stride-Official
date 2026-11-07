#ifndef STRIDE_DEX_FFI_H
#define STRIDE_DEX_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct StrideDexSummary {
    uint32_t class_count;
    uint32_t method_count;
} StrideDexSummary;

/*
 * Scan one DEX file and return class/method counts.
 * Returns 0 on success; 1 for null pointers; 2 for an oversized buffer;
 * 3 for invalid DEX; 4 for an internal parser panic; 5 for count overflow.
 *
 * data must reference len readable bytes and out must reference one writable
 * StrideDexSummary for the duration of the call.
 */
int32_t stride_dex_inventory_counts(
    const uint8_t *data,
    size_t len,
    StrideDexSummary *out
);

#ifdef __cplusplus
}
#endif

#endif
