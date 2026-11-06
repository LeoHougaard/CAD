// Exact FreeCAD 1.1.4 Linux ABI access, with no geometry/constraint implementation.
// Generated symbols must match the complete native declarations in the builder.
#include <dlfcn.h>
#include <vector>
#include <exception>
#include <stdexcept>
#include <cstdio>
#include "mirror_symbols.h"

namespace Sketcher { enum class PointPos : int; }
namespace {
void** application = nullptr;
void* (*get_document)(void*, const char*) = nullptr;
void* (*get_object)(void*, const char*) = nullptr;
int (*add_symmetric)(void*, const std::vector<int>&, int, Sketcher::PointPos, bool) = nullptr;
}

extern "C" __attribute__((visibility("default"))) int mirror_initialize(
    void* app_library, void* sketcher_library) noexcept
{
    if (!app_library || !sketcher_library) return 0;
    auto singleton = reinterpret_cast<void**>(dlsym(app_library, MIRROR_APPLICATION));
    auto document = reinterpret_cast<decltype(get_document)>(dlsym(app_library, MIRROR_DOCUMENT));
    auto object = reinterpret_cast<decltype(get_object)>(dlsym(app_library, MIRROR_OBJECT));
    auto symmetric = reinterpret_cast<decltype(add_symmetric)>(dlsym(sketcher_library, MIRROR_SYMMETRIC));
    if (!singleton || !document || !object || !symmetric) return 0;
    application = singleton;
    get_document = document;
    get_object = object;
    add_symmetric = symmetric;
    return 1;
}

extern "C" __attribute__((visibility("default"))) int mirror_native(
    const char* document, const char* object, const int* indices, int count,
    int reference, int point, bool constraints, char* error, int capacity) noexcept
{
    if (error && capacity > 0) error[0] = '\0';
    try {
        if (!application || !*application || !get_document || !get_object || !add_symmetric)
            throw std::runtime_error("Pinned FreeCAD Mirror exports unavailable");
        if (!document || !object || !indices || count < 1 || point < 0 || point > 3)
            throw std::runtime_error("Native Mirror object or selection unavailable");
        void* doc = get_document(*application, document);
        void* obj = doc ? get_object(doc, object) : nullptr;
        if (!obj) throw std::runtime_error("Native Mirror object or selection unavailable");
        // Python validates the live native SketchObject under the GIL. Pinned
        // SketchObject -> Part2DObject -> Feature -> GeoFeature -> DocumentObject
        // follows the first dynamic primary base at offset zero. Part2DObject's
        // secondary AttachExtension is never used for this pointer conversion.
        std::vector<int> selected(indices, indices + count);
        return add_symmetric(obj, selected, reference, static_cast<Sketcher::PointPos>(point), constraints);
    } catch (const std::exception& e) {
        if (error && capacity > 0) snprintf(error, capacity, "%s", e.what());
    } catch (...) {
        if (error && capacity > 0) snprintf(error, capacity, "%s", "Native FreeCAD symmetry failed");
    }
    return -2;
}
