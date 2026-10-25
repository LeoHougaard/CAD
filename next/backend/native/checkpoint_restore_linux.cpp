// FreeCAD 1.1.4 Linux / Itanium C++ ABI. Symbols are generated from the actual
// pinned ELF exports after checking their complete demangled declarations.
#include <dlfcn.h>
#include <sstream>
#include <string>
#include <cstdio>
#include "checkpoint_symbols.h"

namespace {
void** application = nullptr;
void* (*get_document)(void*, const char*) = nullptr;
void* (*get_object)(void*, const char*) = nullptr;
void (*restore_stream)(void*, std::istream&) = nullptr;
}
extern "C" __attribute__((visibility("default"))) int checkpoint_initialize(
    void* app_library, void* base_library) noexcept
{
    application = reinterpret_cast<void**>(dlsym(app_library, CHECKPOINT_APPLICATION));
    get_document = reinterpret_cast<void* (*)(void*, const char*)>(dlsym(app_library, CHECKPOINT_DOCUMENT));
    get_object = reinterpret_cast<void* (*)(void*, const char*)>(dlsym(app_library, CHECKPOINT_OBJECT));
    restore_stream = reinterpret_cast<void (*)(void*, std::istream&)>(dlsym(base_library, CHECKPOINT_RESTORE));
    return application && get_document && get_object && restore_stream;
}
extern "C" __attribute__((visibility("default"))) int checkpoint_restore_native(
    const char* document, const char* object, const char* data, size_t length,
    char* error, int capacity) noexcept
{
    try {
        if (!application || !*application || !get_document || !get_object || !restore_stream || !data)
            throw 0;
        void* doc = get_document(*application, document);
        void* obj = doc ? get_object(doc, object) : nullptr;
        if (!obj) throw 0;
        // Verified single primary base chain from DocumentObject through
        // TransactionalObject/ExtensionContainer/PropertyContainer to Persistence.
        // The Python boundary verifies the live native object under the GIL.
        std::istringstream input(std::string(data, length), std::ios::in | std::ios::binary);
        restore_stream(obj, input);
        return 1;
    } catch (...) {
        snprintf(error, capacity, "%s", "Unable to restore content");
        return 0;
    }
}
