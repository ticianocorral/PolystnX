/* Frontend-owned shim: exposes the size of the opaque filter table so Rust can
   heap-allocate it without hard-coding a platform-dependent number. */
#include <stddef.h>
#include "snes_ntsc.h"

size_t xperience_snes_ntsc_sizeof(void) {
    return sizeof(struct snes_ntsc_t);
}
