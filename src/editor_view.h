#pragma once

#include "project.h"

#include <wx/control.h>
#include <wx/vlbox.h>

#include <functional>

namespace meditor::view {
inline const wxColour canvas("#101319");
inline const wxColour surface("#181D26");
inline const wxColour field("#1F2632");
inline const wxColour line("#2B3443");
inline const wxColour text("#EDF2F9");
inline const wxColour muted("#A7B4C9");
inline const wxColour accent("#6BEAC7");
inline const wxColour ink("#12223A");

wxFont font(int points, bool bold = false);
wxString timeText(Time time);
void useDarkTitleBar(wxWindow& window);

class CommandButton : public wxControl {
public:
    CommandButton(wxWindow* parent, const wxString& label, int width, bool primary = false);

    void SetLabel(const wxString& label) override;
    bool Enable(bool enable = true) override;
    std::function<void()> action;

private:
    void paint(wxPaintEvent&);
    bool primary_;
    bool hovered_ = false;
    bool pressed_ = false;
};

class CaptionList : public wxVListBox {
public:
    explicit CaptionList(wxWindow* parent);
    void reset(const Project& project);

private:
    void OnDrawItem(wxDC& dc, const wxRect& rect, std::size_t row) const override;
    void OnDrawBackground(wxDC& dc, const wxRect& rect, std::size_t row) const override;
    wxCoord OnMeasureItem(std::size_t) const override;

    const Project* project_ = nullptr;
    wxFont detailFont_;
};

class ScrubTimeline : public wxControl {
public:
    static constexpr int range = 1'000'000;

    explicit ScrubTimeline(wxWindow* parent);
    void setValue(int value);
    void setDuration(Time duration);
    std::function<void(int)> track;
    std::function<void(int)> release;
    std::function<void(int)> seek;

private:
    void paint(wxPaintEvent&);
    int valueAt(int x) const;
    void trackAt(int x);
    void finish();

    int value_ = 0;
    Time duration_ = 0;
    bool dragging_ = false;
};
}
