#pragma once

class wxString;
class wxWindow;

namespace meditor {
// Capture just the editor window and verify that the synthetic preview is visible.
bool saveWindowsScreenshot(wxWindow& window, const wxString& destination);
}
