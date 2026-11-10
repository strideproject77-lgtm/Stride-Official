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
    float average_fps;
    uint32_t heap_allocated_bytes;
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
 * Load and link an ELF native library (.so) into the active runtime.
 */
int32_t stride_runtime_load_so(
    AndroidRuntime *runtime,
    const char *soname,
    const uint8_t *data,
    size_t len
);

/*
 * Read the launch Activity's AndroidManifest.xml screenOrientation enum.
 * Returns 0 on success, 1 for invalid pointers, or 2 for invalid manifest data.
 */
int32_t stride_manifest_screen_orientation(const uint8_t *manifest_data, size_t manifest_len, int32_t *out_orientation);

/*
 * Dispatch the launch Activity lifecycle from AndroidManifest.xml.
 * out_report.status_code: 0=active guest runtime (reserved), 1=finished,
 * 2=uncaught exception, 3=activity not found, 4=manifest error,
 * 5=lifecycle callbacks returned but no persistent guest loop is active.
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
 * Replace text in the currently focused guest EditText and refresh its frame.
 * Returns 0 on success, 1 for invalid arguments, 2 for guest error,
 * 3 for runtime panic, and 4 when no EditText is focused.
 */
int32_t stride_runtime_set_focused_text(
    AndroidRuntime *runtime,
    const uint8_t *text,
    size_t length
);

/*
 * Pump queued Android touch events into the active Activity callback.
 * Returns 0 on success, 1 for invalid arguments, 2 for guest callback error,
 * and 3 for a runtime panic. out_delivered receives the delivered event count.
 */
int32_t stride_runtime_pump_guest_events(
    AndroidRuntime *runtime,
    uint32_t max_events,
    size_t step_budget,
    uint32_t *out_delivered
);

/*
 * Record a completed frame duration (ms) and return current rolling FPS.
 */
float stride_runtime_record_frame(AndroidRuntime *runtime, float duration_ms);

/*
 * Swap active OpenGL ES buffers into presentation surface.
 */
int32_t stride_runtime_gl_swap_buffers(AndroidRuntime *runtime);

/*
 * Set the OpenGL ES viewport.
 */
int32_t stride_runtime_gl_viewport(
    AndroidRuntime *runtime,
    int32_t x,
    int32_t y,
    int32_t width,
    int32_t height
);

/*
 * Clear the OpenGL ES color buffer.
 */
int32_t stride_runtime_gl_clear(
    AndroidRuntime *runtime,
    float r,
    float g,
    float b,
    float a
);

/*
 * Export a UTF-8 diagnostics report. Returns required capacity including NUL.
 * Call with out=NULL/capacity=0 to query size, then provide a buffer.
 */
size_t stride_runtime_diagnostics_export(
    const AndroidRuntime *runtime,
    char *out,
    size_t capacity
);

/*
 * Destroy the runtime instance and free its resources.
 */
void stride_runtime_destroy(AndroidRuntime *runtime);


int32_t stride_runtime_copy_framebuffer(const AndroidRuntime *runtime, uint32_t *out_width, uint32_t *out_height, uint32_t *out_pixels, size_t capacity_pixels);
int32_t stride_runtime_load_resource_table(AndroidRuntime *runtime, const uint8_t *data, size_t len);
size_t stride_runtime_resource_file_count(const AndroidRuntime *runtime);
size_t stride_runtime_resource_file_entry(const AndroidRuntime *runtime, size_t index, uint32_t *out_id, uint8_t *out_path, size_t capacity);
int32_t stride_runtime_set_resource_image(AndroidRuntime *runtime, uint32_t id, uint32_t width, uint32_t height, const uint32_t *pixels, size_t count);

/* Unity game bridge */
int32_t stride_runtime_unity_step_frame(AndroidRuntime *runtime);
int32_t stride_runtime_unity_send_message(AndroidRuntime *runtime, const char *game_object, const char *method_name, const char *message);
int32_t stride_runtime_unity_resume(AndroidRuntime *runtime);
int32_t stride_runtime_unity_pause(AndroidRuntime *runtime);

#ifdef __cplusplus
}
#endif

#endif
