// Pinned FreeCAD 1.1.4 / release MSVC 14.44 Windows x64 adapter.
// Call the same Persistence::restoreFromStream as restoreContent, bypassing
// the Python binding's unreleased Py_buffer. No Python reference surgery.
#include <windows.h>
#include <sstream>
#include <string>
#include <stdexcept>
#include <cstdio>

extern "C" __declspec(dllexport) int checkpoint_restore_native(
    const char* document, const char* object, const char* data, size_t length,
    char* error, int capacity) noexcept
{
    try {
        auto app = GetModuleHandleW(L"FreeCADApp.dll");
        auto base = GetModuleHandleW(L"FreeCADBase.dll");
        auto singleton = reinterpret_cast<void**>(GetProcAddress(app,
            "?_pcSingleton@Application@App@@0PEAV12@EA"));
        auto getDocument = reinterpret_cast<void* (*)(void*, const char*)>(GetProcAddress(app,
            "?getDocument@Application@App@@QEBAPEAVDocument@2@PEBD@Z"));
        auto getObject = reinterpret_cast<void* (*)(void*, const char*)>(GetProcAddress(app,
            "?getObject@Document@App@@QEBAPEAVDocumentObject@2@PEBD@Z"));
        auto restore = reinterpret_cast<void (*)(void*, std::istream&)>(GetProcAddress(base,
            "?restoreFromStream@Persistence@Base@@QEAAXAEAV?$basic_istream@DU?$char_traits@D@std@@@std@@@Z"));
        if (!singleton || !*singleton || !getDocument || !getObject || !restore || !data)
            throw std::runtime_error("Pinned native checkpoint exports/input unavailable");
        void* doc = getDocument(*singleton, document);
        void* obj = doc ? getObject(doc, object) : nullptr;
        if (!obj) throw std::runtime_error("Native checkpoint object unavailable");
        // Verified primary-base chain: DocumentObject -> TransactionalObject ->
        // ExtensionContainer -> PropertyContainer -> Base::Persistence.
        // Python validates a live native SketchObject in this exact document.
        // std::istringstream owns its temporary copy and releases it on every exit.
        std::istringstream stream(std::string(data, length), std::ios::in | std::ios::binary);
        restore(obj, stream);
        return 1;
    } catch (...) {
        // Match the native Python binding's exception type/message. The caller
        // raises OSError, and existing worker recovery/poisoning remains in force.
        snprintf(error, capacity, "%s", "Unable to restore content");
        return 0;
    }
}
