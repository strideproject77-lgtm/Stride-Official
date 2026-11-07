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

typedef struct StrideLaunchReport {
    int32_t status_code;
    uint32_t total_classes_loaded;
    uint32_t instructions_executed;
    int32_t lifecycle_state;
    uint32_t missing_apis_count;
} StrideLaunchReport;

typedef struct AndroidRuntime AndroidRuntime;

/*
 * Scan one DEX file and return class/method counts.
 */
int32_t stride_dex_inventory_counts(
    const uint8_t *data,
    size_t len,
    StrideDexSummary *out
);

/*
 * Create an ART-compatible Android runtime instance.
 */
AndroidRuntime *stride_runtime_create(const char *package_name);

/*
 * Load a DEX binary into the active runtime.
 */
int32_t stride_runtime_load_dex(
    AndroidRuntime *runtime,
    const uint8_t *data,
    size_t len
);

/*
 * Launch the application from AndroidManifest.xml and execute its main activity.
 */
int32_t stride_runtime_launch_apk(
    AndroidRuntime *runtime,
    const uint8_t *manifest_data,
    size_t manifest_len,
    size_t step_budget,
    StrideLaunchReport *out_report
);

/*
 * Dispatch a touch event to the active runtime input queue.
 */
int32_t stride_runtime_dispatch_touch(
    AndroidRuntime *runtime,
    int32_t action,
    float x,
    float y
);

/*
 * Destroy the runtime instance and free its resources.
 */
void stride_runtime_destroy(AndroidRuntime *runtime);

#ifdef __cplusplus
}
#endif

#endif
