/* Compile-time truth for Rust: the byte offset of `state` inside
 * `struct rc_trigger_t` (rc_runtime_types.h), so the Rust side never
 * hardcodes a layout guess. */
#include <stddef.h>
#include "rc_runtime_types.h"

uint32_t ra_trigger_state_offset(void) {
  return (uint32_t)offsetof(struct rc_trigger_t, state);
}
