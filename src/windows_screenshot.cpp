#include "windows_screenshot.h"

#ifndef NOMINMAX
#define NOMINMAX
#endif

#include <wx/image.h>
#include <wx/msw/wrapwin.h>
#include <wx/window.h>

#include <algorithm>
#include <cstddef>

namespace meditor {
namespace {
struct CaptureSurface {
    HDC dc = CreateCompatibleDC(nullptr);
    HBITMAP bitmap = nullptr;
    HGDIOBJ previous = nullptr;

    ~CaptureSurface()
    {
        if (previous && previous != HGDI_ERROR)
            SelectObject(dc, previous);
        if (bitmap)
            DeleteObject(bitmap);
        if (dc)
            DeleteDC(dc);
    }
};

bool showsSyntheticPreview(const unsigned char* pixels, int width, int height)
{
    int colorfulSamples = 0;
    for (int y = height / 3; y < height * 2 / 3; y += 8) {
        for (int x = width / 8; x < width * 2 / 5; x += 8) {
            const auto* p = pixels + (static_cast<std::size_t>(y) * width + x) * 4;
            if (std::max({p[0], p[1], p[2]}) - std::min({p[0], p[1], p[2]}) >= 100 &&
                ++colorfulSamples == 16)
                return true;
        }
    }
    return false;
}
}

bool saveWindowsScreenshot(wxWindow& window, const wxString& destination)
{
    const auto handle = static_cast<HWND>(window.GetHandle());
    RECT bounds{};
    if (!GetWindowRect(handle, &bounds))
        return false;

    const int width = bounds.right - bounds.left;
    const int height = bounds.bottom - bounds.top;
    if (width <= 0 || height <= 0 || width > 8192 || height > 8192)
        return false;

    CaptureSurface surface;
    if (!surface.dc)
        return false;

    BITMAPINFO info{};
    info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    void* data = nullptr;
    surface.bitmap = CreateDIBSection(surface.dc, &info, DIB_RGB_COLORS, &data, nullptr, 0);
    if (!surface.bitmap || !data)
        return false;
    surface.previous = SelectObject(surface.dc, surface.bitmap);
    if (!surface.previous || surface.previous == HGDI_ERROR)
        return false;

    auto* pixels = static_cast<unsigned char*>(data);
    const auto size = static_cast<std::size_t>(width) * height;
    auto print = [&](UINT flags) {
        std::fill_n(pixels, size * 4, 0);
        const bool copied = PrintWindow(handle, surface.dc, flags);
        GdiFlush();
        return copied && showsSyntheticPreview(pixels, width, height);
    };
    if (!print(PW_RENDERFULLCONTENT) && !print(0))
        return false;

    wxImage image(width, height, false);
    if (!image.IsOk())
        return false;
    auto* rgb = image.GetData();
    for (std::size_t i = 0; i < size; ++i) {
        rgb[3 * i] = pixels[4 * i + 2];
        rgb[3 * i + 1] = pixels[4 * i + 1];
        rgb[3 * i + 2] = pixels[4 * i];
    }
    wxInitAllImageHandlers();
    return image.SaveFile(destination, wxBITMAP_TYPE_PNG);
}
}
