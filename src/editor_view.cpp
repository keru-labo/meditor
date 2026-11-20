#include "editor_view.h"

#include <wx/dcbuffer.h>
#include <wx/settings.h>

#if wxUSE_ACCESSIBILITY
#include <wx/access.h>
#endif

#ifdef __WXMSW__
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <wx/msw/wrapwin.h>
#include <dwmapi.h>
#endif

#include <algorithm>
#include <array>

namespace meditor::view {
namespace {
#if wxUSE_ACCESSIBILITY
class ControlAccessible : public wxAccessible {
public:
    ControlAccessible(wxWindow* window, wxAccRole role) : wxAccessible(window), role_(role) {}

    std::function<void()> activate;
    std::function<wxString()> value;

    wxAccStatus GetName(int child, wxString* name) override
    {
        if (child != wxACC_SELF)
            return wxACC_FAIL;
        *name = GetWindow()->GetName();
        return wxACC_OK;
    }

    wxAccStatus GetRole(int child, wxAccRole* role) override
    {
        if (child != wxACC_SELF)
            return wxACC_FAIL;
        *role = role_;
        return wxACC_OK;
    }

    wxAccStatus GetState(int child, long* state) override
    {
        if (child != wxACC_SELF)
            return wxACC_FAIL;
        const auto* window = GetWindow();
        *state = window->IsEnabled() ? wxACC_STATE_SYSTEM_FOCUSABLE : wxACC_STATE_SYSTEM_UNAVAILABLE;
        if (window->HasFocus())
            *state |= wxACC_STATE_SYSTEM_FOCUSED;
        return wxACC_OK;
    }

    wxAccStatus GetValue(int child, wxString* stringValue) override
    {
        if (child != wxACC_SELF || !value)
            return wxACC_NOT_SUPPORTED;
        *stringValue = value();
        return wxACC_OK;
    }

    wxAccStatus GetDefaultAction(int child, wxString* name) override
    {
        if (child != wxACC_SELF)
            return wxACC_FAIL;
        *name = activate ? wxString(L"押す") : wxString{};
        return wxACC_OK;
    }

    wxAccStatus DoDefaultAction(int child) override
    {
        if (child != wxACC_SELF || !activate || !GetWindow()->IsEnabled())
            return wxACC_NOT_SUPPORTED;
        activate();
        return wxACC_OK;
    }

private:
    wxAccRole role_;
};
#endif

void box(wxDC& dc, const wxRect& bounds, const wxColour& fill, int radius)
{
    dc.SetPen(*wxTRANSPARENT_PEN);
    dc.SetBrush(wxBrush(fill));
    dc.DrawRoundedRectangle(bounds, radius);
}

wxString display(std::string_view text)
{
    return wxString::FromUTF8(text.data(), text.size());
}
}

wxFont font(int points, bool bold)
{
    auto result = wxSystemSettings::GetFont(wxSYS_DEFAULT_GUI_FONT);
    result.SetPointSize(points);
    if (bold)
        result.MakeBold();
    return result;
}

wxString timeText(Time time)
{
    return wxString::Format("%02lld:%02lld.%03lld",
        static_cast<long long>(time / (60 * second)),
        static_cast<long long>(time / second % 60),
        static_cast<long long>(time / 1000 % 1000));
}

void useDarkTitleBar(wxWindow& window)
{
#ifdef __WXMSW__
    constexpr DWORD immersiveDarkMode = 20;
    const BOOL enabled = TRUE;
    DwmSetWindowAttribute(static_cast<HWND>(window.GetHandle()), immersiveDarkMode,
                         &enabled, sizeof(enabled));
#else
    static_cast<void>(window);
#endif
}

CommandButton::CommandButton(wxWindow* parent, const wxString& label, int width, bool primary)
    : wxControl(parent, wxID_ANY, wxDefaultPosition, wxDefaultSize, wxBORDER_NONE),
      primary_(primary)
{
    SetBackgroundStyle(wxBG_STYLE_PAINT);
    SetMinSize(FromDIP(wxSize(width, 34)));
    SetLabel(label);
    SetFont(font(10, true));
#if wxUSE_ACCESSIBILITY
    auto* accessible = new ControlAccessible(this, wxROLE_SYSTEM_PUSHBUTTON);
    accessible->activate = [this] { if (action) action(); };
    SetAccessible(accessible);
#endif
    Bind(wxEVT_PAINT, &CommandButton::paint, this);
    Bind(wxEVT_ENTER_WINDOW, [this](wxMouseEvent&) { hovered_ = true; Refresh(false); });
    Bind(wxEVT_LEAVE_WINDOW, [this](wxMouseEvent&) { hovered_ = false; Refresh(false); });
    Bind(wxEVT_SET_FOCUS, [this](wxFocusEvent& event) { Refresh(false); event.Skip(); });
    Bind(wxEVT_KILL_FOCUS, [this](wxFocusEvent& event) { Refresh(false); event.Skip(); });
    Bind(wxEVT_LEFT_DOWN, [this](wxMouseEvent&) {
        SetFocus();
        CaptureMouse();
        pressed_ = true;
        Refresh(false);
    });
    Bind(wxEVT_LEFT_UP, [this](wxMouseEvent& event) {
        if (!pressed_)
            return;
        pressed_ = false;
        if (HasCapture())
            ReleaseMouse();
        Refresh(false);
        if (wxRect(GetClientSize()).Contains(event.GetPosition()) && action)
            action();
    });
    Bind(wxEVT_MOUSE_CAPTURE_LOST, [this](wxMouseCaptureLostEvent&) {
        pressed_ = false;
        Refresh(false);
    });
    Bind(wxEVT_KEY_DOWN, [this](wxKeyEvent& event) {
        if (event.GetKeyCode() == WXK_RETURN || event.GetKeyCode() == WXK_SPACE) {
            if (action)
                action();
        } else {
            event.Skip();
        }
    });
}

void CommandButton::SetLabel(const wxString& label)
{
    wxControl::SetLabel(label);
    SetName(label);
    Refresh(false);
}

bool CommandButton::Enable(bool enable)
{
    const auto changed = wxControl::Enable(enable);
    if (changed)
        Refresh(false);
    return changed;
}

void CommandButton::paint(wxPaintEvent&)
{
    wxAutoBufferedPaintDC dc(this);
    dc.SetBackground(wxBrush(GetParent()->GetBackgroundColour()));
    dc.Clear();
    const wxRect bounds(GetClientSize());
    const auto fill = !IsEnabled() ? line :
                      primary_ ? (pressed_ ? wxColour("#50C6A7") : hovered_ ? wxColour("#A1F7E0") : accent) :
                                 (hovered_ ? line : field);
    box(dc, bounds, fill, FromDIP(7));
    if (HasFocus()) {
        dc.SetPen(wxPen(primary_ ? ink : accent));
        dc.SetBrush(*wxTRANSPARENT_BRUSH);
        auto focus = bounds;
        focus.Deflate(FromDIP(2));
        dc.DrawRoundedRectangle(focus, FromDIP(5));
    }
    dc.SetFont(GetFont());
    dc.SetTextForeground(!IsEnabled() ? muted : primary_ ? ink : text);
    dc.DrawLabel(GetLabel(), bounds, wxALIGN_CENTER);
}

CaptionList::CaptionList(wxWindow* parent)
    : wxVListBox(parent, wxID_ANY, wxDefaultPosition, wxDefaultSize, wxBORDER_NONE),
      detailFont_(font(9))
{
    SetBackgroundColour(surface);
    SetForegroundColour(text);
    SetFont(font(10));
    SetName(L"字幕一覧");
}

void CaptionList::reset(const Project& project)
{
    SetSelection(wxNOT_FOUND);
    project_ = &project;
    SetItemCount(project.captions.size());
    Refresh(false);
}

wxCoord CaptionList::OnMeasureItem(std::size_t) const
{
    return FromDIP(33);
}

void CaptionList::OnDrawBackground(wxDC& dc, const wxRect& row, std::size_t index) const
{
    if (IsSelected(index)) {
        auto bounds = row;
        bounds.Deflate(FromDIP(1), FromDIP(2));
        box(dc, bounds, wxColour("#243B3E"), FromDIP(6));
        box(dc, wxRect(bounds.x + FromDIP(3), bounds.y + FromDIP(5),
                       FromDIP(3), bounds.height - FromDIP(10)), accent, FromDIP(2));
    } else {
        dc.SetPen(wxPen(line));
        dc.DrawLine(row.x + FromDIP(10), row.GetBottom(), row.GetRight(), row.GetBottom());
    }
}

void CaptionList::OnDrawItem(wxDC& dc, const wxRect& row, std::size_t index) const
{
    const auto& caption = project_->captions[index];
    const int padding = FromDIP(12);
    const int speakerStart = row.x + FromDIP(100);
    const int captionStart = row.x + FromDIP(179);
    dc.SetFont(detailFont_);
    dc.SetTextForeground(muted);
    dc.DrawLabel(timeText(caption.start), wxRect(row.x + padding, row.y,
                 speakerStart - row.x - padding, row.height), wxALIGN_CENTER_VERTICAL);

    if (!caption.speaker.empty()) {
        const auto tag = display(caption.speaker);
        const int maxText = FromDIP(54);
        const wxRect badge(speakerStart, row.y + (row.height - FromDIP(21)) / 2,
                           std::min(dc.GetTextExtent(tag).x, maxText) + FromDIP(14), FromDIP(21));
        box(dc, badge, wxColour("#293747"), FromDIP(5));
        dc.SetTextForeground(wxColour("#BACCE4"));
        dc.DrawLabel(wxControl::Ellipsize(tag, dc, wxELLIPSIZE_END, maxText), badge, wxALIGN_CENTER);
    }
    const int remaining = row.GetRight() - captionStart - padding;
    if (remaining <= 1)
        return;
    auto title = display(caption.text);
    title.Replace("\n", " ");
    dc.SetFont(GetFont());
    dc.SetTextForeground(text);
    dc.DrawLabel(wxControl::Ellipsize(title, dc, wxELLIPSIZE_END, remaining),
                 wxRect(captionStart, row.y, remaining, row.height), wxALIGN_CENTER_VERTICAL);
}

ScrubTimeline::ScrubTimeline(wxWindow* parent)
    : wxControl(parent, wxID_ANY, wxDefaultPosition, wxDefaultSize, wxBORDER_NONE)
{
    SetMinSize(FromDIP(wxSize(-1, 46)));
    SetBackgroundStyle(wxBG_STYLE_PAINT);
    SetName(L"タイムライン");
    SetFont(font(8));
#if wxUSE_ACCESSIBILITY
    auto* accessible = new ControlAccessible(this, wxROLE_SYSTEM_SLIDER);
    accessible->value = [this] { return timeText(duration_ * value_ / range); };
    SetAccessible(accessible);
#endif
    Bind(wxEVT_PAINT, &ScrubTimeline::paint, this);
    Bind(wxEVT_SIZE, [this](wxSizeEvent& event) { Refresh(false); event.Skip(); });
    Bind(wxEVT_SET_FOCUS, [this](wxFocusEvent& event) { Refresh(false); event.Skip(); });
    Bind(wxEVT_KILL_FOCUS, [this](wxFocusEvent& event) { Refresh(false); event.Skip(); });
    Bind(wxEVT_LEFT_DOWN, [this](wxMouseEvent& event) {
        SetFocus();
        CaptureMouse();
        dragging_ = true;
        trackAt(event.GetX());
    });
    Bind(wxEVT_MOTION, [this](wxMouseEvent& event) {
        if (dragging_)
            trackAt(event.GetX());
    });
    Bind(wxEVT_LEFT_UP, [this](wxMouseEvent& event) {
        if (dragging_) {
            trackAt(event.GetX());
            finish();
        }
    });
    Bind(wxEVT_MOUSE_CAPTURE_LOST, [this](wxMouseCaptureLostEvent&) { finish(); });
    Bind(wxEVT_KEY_DOWN, [this](wxKeyEvent& event) {
        const int key = event.GetKeyCode();
        if (key != WXK_LEFT && key != WXK_RIGHT && key != WXK_HOME && key != WXK_END) {
            event.Skip();
            return;
        }
        const int step = event.ShiftDown() ? range / 20 : range / 200;
        setValue(key == WXK_HOME ? 0 : key == WXK_END ? range : value_ + (key == WXK_RIGHT ? step : -step));
        if (seek)
            seek(value_);
    });
}

void ScrubTimeline::setValue(int value)
{
    value = std::clamp(value, 0, range);
    if (value != value_) {
        value_ = value;
        Refresh(false);
    }
}

void ScrubTimeline::setDuration(Time duration)
{
    duration_ = duration;
    Refresh(false);
}

int ScrubTimeline::valueAt(int x) const
{
    const int left = FromDIP(12);
    const int width = std::max(1, GetClientSize().x - left * 2);
    const auto offset = static_cast<Time>(std::clamp(x - left, 0, width));
    return static_cast<int>(offset * range / width);
}

void ScrubTimeline::trackAt(int x)
{
    setValue(valueAt(x));
    if (track)
        track(value_);
}

void ScrubTimeline::finish()
{
    if (!dragging_)
        return;
    dragging_ = false;
    if (HasCapture())
        ReleaseMouse();
    if (release)
        release(value_);
}

void ScrubTimeline::paint(wxPaintEvent&)
{
    wxAutoBufferedPaintDC dc(this);
    dc.SetBackground(wxBrush(surface));
    dc.Clear();
    const int left = FromDIP(12);
    const int width = std::max(1, GetClientSize().x - left * 2);
    const int rail = FromDIP(29);
    dc.SetFont(GetFont());
    dc.SetTextForeground(muted);
    dc.SetPen(wxPen(line));
    if (duration_ > 0) {
        constexpr std::array<Time, 18> intervals{
            100'000, 200'000, 500'000, second, 2 * second, 5 * second,
            10 * second, 15 * second, 30 * second, 60 * second, 2 * 60 * second,
            5 * 60 * second, 10 * 60 * second, 15 * 60 * second, 30 * 60 * second,
            3600 * second, 2 * 3600 * second, 4 * 3600 * second
        };
        const auto desired = duration_ / std::max(2, width / FromDIP(102));
        const auto found = std::lower_bound(intervals.begin(), intervals.end(), desired);
        const Time major = found == intervals.end() ? intervals.back() : *found;
        const Time minor = major / 5;
        for (Time tick = 0; tick <= duration_; tick += minor) {
            const int x = left + static_cast<int>(width * tick / duration_);
            const bool labelled = tick % major == 0;
            dc.DrawLine(x, FromDIP(19), x, labelled ? FromDIP(25) : FromDIP(22));
            if (labelled) {
                const auto formatted = timeText(tick);
                const auto label = major < second ? formatted.Left(7) : formatted.BeforeFirst('.');
                const int labelWidth = dc.GetTextExtent(label).x;
                dc.DrawText(label, std::clamp(x - labelWidth / 2, 0,
                           std::max(0, GetClientSize().x - labelWidth)), FromDIP(2));
            }
        }
    }
    box(dc, wxRect(left, rail, width, FromDIP(8)), field, FromDIP(4));
    if (duration_ == 0)
        return;
    const int run = static_cast<int>(static_cast<Time>(width) * value_ / range);
    if (run)
        box(dc, wxRect(left, rail, run, FromDIP(8)), wxColour("#337D75"), FromDIP(4));
    const int x = left + run;
    dc.SetPen(wxPen(HasFocus() ? text : accent, FromDIP(2)));
    dc.DrawLine(x, FromDIP(19), x, FromDIP(44));
    box(dc, wxRect(x - FromDIP(4), rail - FromDIP(2), FromDIP(8), FromDIP(12)), accent, FromDIP(3));
}
}
