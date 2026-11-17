// fit.cpp's common_measure_memory() calls three functions of common.cpp, which is not built in (it brings the rest of
// llama.cpp's common library, downloads included). lastbook-llm never calls common_measure_memory(). The macOS linker drops it
// as dead code; the Windows linker resolves every reference first, so these stand in and stop the program if ever reached.
#include "common.h"

#include <cstdio>
#include <cstdlib>

static void unused(const char * name) {
    fprintf(stderr, "lastbook-llm: %s is not part of this program\n", name);
    abort();
}

struct llama_model_params common_model_params_to_llama(common_params &) {
    unused("common_model_params_to_llama");
    return {};
}

struct llama_context_params common_context_params_to_llama(const common_params &) {
    unused("common_context_params_to_llama");
    return {};
}

void common_fit_apply(common_params &, struct llama_model_params &, struct llama_context_params &) {
    unused("common_fit_apply");
}
