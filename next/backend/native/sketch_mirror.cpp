// Pinned Windows x64 ABI adapter. No geometry or constraint implementation.
// FreeCAD 1.1.4's Python binding omits the final addSymmetric boolean.
// Resolve the same native SketchObject method used by DrawSketchHandlerSymmetry.
#include <windows.h>
#include <vector>
#include <exception>
#include <stdexcept>
#include <cstdio>

extern "C" __declspec(dllexport) int mirror_native(
    const char* document, const char* object, const int* indices, int count,
    int reference, int point, bool constraints, char* error, int capacity) noexcept
{
    try {
        auto app = GetModuleHandleW(L"FreeCADApp.dll");
        auto sketcher = GetModuleHandleW(L"Sketcher.pyd");
        auto singleton = reinterpret_cast<void**>(GetProcAddress(app,
            "?_pcSingleton@Application@App@@0PEAV12@EA"));
        auto getDocument = reinterpret_cast<void* (*)(void*, const char*)>(GetProcAddress(app,
            "?getDocument@Application@App@@QEBAPEAVDocument@2@PEBD@Z"));
        auto getObject = reinterpret_cast<void* (*)(void*, const char*)>(GetProcAddress(app,
            "?getObject@Document@App@@QEBAPEAVDocumentObject@2@PEBD@Z"));
        auto symmetric = reinterpret_cast<int (*)(void*, const std::vector<int>&, int, int, bool)>(
            GetProcAddress(sketcher,
            "?addSymmetric@SketchObject@Sketcher@@QEAAHAEBV?$vector@HV?$allocator@H@std@@@std@@HW4PointPos@2@_N@Z"));
        if (!singleton || !*singleton || !getDocument || !getObject || !symmetric)
            throw std::runtime_error("Pinned FreeCAD Mirror exports unavailable");
        void* doc = getDocument(*singleton, document);
        void* obj = doc ? getObject(doc, object) : nullptr;
        if (!obj || count < 1) throw std::runtime_error("Native Mirror object or selection unavailable");
        // SketchObject -> Part2DObject -> Feature -> GeoFeature -> DocumentObject is the
        // primary base chain in this exact binary. Python validates the type.
        std::vector<int> selected(indices, indices + count);
        return symmetric(obj, selected, reference, point, constraints);
    } catch (const std::exception& e) {
        snprintf(error, capacity, "%s", e.what());
    } catch (...) {
        snprintf(error, capacity, "%s", "Native FreeCAD symmetry failed");
    }
    return -2;
}
