#include "audio.h"
#include "editor_view.h"
#include "project.h"
#include "video.h"

#include <wx/wx.h>
#include <wx/dcbuffer.h>
#include <wx/splitter.h>

#ifdef __WXMSW__
#include "windows_screenshot.h"
#endif

#include <algorithm>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <functional>
#include <nlohmann/json.hpp>
#include <utility>

namespace meditor {
namespace {
wxString display(const std::string& text) { return wxString::FromUTF8(text); }
std::string utf8(const wxString& text)
{
    const auto bytes = text.ToUTF8();
    return bytes.data() ? std::string(bytes.data(), bytes.length()) : std::string{};
}

std::filesystem::path filePath(const wxString& text)
{
#ifdef _WIN32
    return text.ToStdWstring();
#else
    return fromUtf8(utf8(text));
#endif
}

using view::CaptionList;
using view::CommandButton;
using view::ScrubTimeline;

wxStaticText* label(wxWindow* parent, const wxString& value, int points,
                    const wxColour& color, bool bold = false, long style = 0)
{
    auto* result = new wxStaticText(parent, wxID_ANY, value, wxDefaultPosition, wxDefaultSize, style);
    result->SetFont(view::font(points, bold));
    result->SetForegroundColour(color);
    return result;
}

wxPanel* panel(wxWindow* parent, const wxColour& background)
{
    auto* result = new wxPanel(parent);
    result->SetBackgroundColour(background);
    return result;
}

wxWindow* paddedInput(wxWindow* parent, wxTextCtrl*& input, int height, long style = 0)
{
    auto* wrapper = panel(parent, view::field);
    auto* layout = new wxBoxSizer(wxVERTICAL);
    input = new wxTextCtrl(wrapper, wxID_ANY, {}, wxDefaultPosition,
                           wrapper->FromDIP(wxSize(-1, height)), wxBORDER_NONE | style);
    input->SetBackgroundColour(view::field);
    input->SetForegroundColour(view::text);
    input->SetFont(view::font(10));
    layout->Add(input, 1, wxEXPAND | wxALL, wrapper->FromDIP(8));
    wrapper->SetSizer(layout);
    return wrapper;
}

class WorkspaceSplit : public wxSplitterWindow {
public:
    explicit WorkspaceSplit(wxWindow* parent)
        : wxSplitterWindow(parent, wxID_ANY, wxDefaultPosition, wxDefaultSize,
                           wxSP_LIVE_UPDATE | wxSP_NOBORDER)
    {
        SetBackgroundColour(view::canvas);
    }

    void DrawSash(wxDC& dc) override
    {
        if (!IsSplit())
            return;
        const int x = GetSashPosition();
        const int width = GetSashSize();
        const int height = GetClientSize().y;
        dc.SetPen(*wxTRANSPARENT_PEN);
        dc.SetBrush(wxBrush(view::canvas));
        dc.DrawRectangle(x, 0, width, height);
        dc.SetPen(wxPen(view::line));
        dc.DrawLine(x + width / 2, height / 2 - FromDIP(14),
                    x + width / 2, height / 2 + FromDIP(14));
    }
};

class Screen : public wxPanel {
public:
    explicit Screen(wxWindow* parent) : wxPanel(parent)
    {
        SetBackgroundStyle(wxBG_STYLE_PAINT);
        Bind(wxEVT_PAINT, &Screen::paint, this);
        Bind(wxEVT_SIZE, [this](wxSizeEvent& event) { resized_ = wxBitmap{}; Refresh(); event.Skip(); });
    }

    void showFrame(const Preview& preview)
    {
        pending_ = preview;
        Refresh(false);
    }

    void clear()
    {
        image_ = {};
        imageFrame_.reset();
        resized_ = wxBitmap{};
        pending_.reset();
        Refresh(false);
    }

    std::function<bool(const VideoRequest&)> accepts;
    std::function<void(VideoRequest, Time)> onPaint;

private:
    void paint(wxPaintEvent&)
    {
        wxAutoBufferedPaintDC dc(this);
        dc.SetBackground(wxBrush(wxColour("#090D14")));
        dc.Clear();
        const auto area = GetClientSize();
        auto candidate = std::exchange(pending_, std::nullopt);
        if (candidate && (!accepts || !accepts(candidate->request)))
            candidate.reset();
        if (candidate && candidate->frame != imageFrame_) {
            imageFrame_ = candidate->frame;
            const auto& frame = *imageFrame_;
            image_.Create(frame.width, frame.height);
            for (int row = 0; row < frame.height; ++row)
                std::memcpy(image_.GetData() + row * frame.width * 3,
                            frame.rgb.data() + row * frame.stride, frame.width * 3);
            resized_ = wxBitmap{};
        }
        if (!image_.IsOk() || area.x < 1 || area.y < 1) {
            dc.SetFont(view::font(11));
            dc.SetTextForeground(view::muted);
            dc.DrawLabel(L"プロジェクトを開くと動画が表示されます", wxRect(area), wxALIGN_CENTER);
            return;
        }
        if (!resized_.IsOk()) {
            const double scale = std::min(double(area.x) / image_.GetWidth(),
                                          double(area.y) / image_.GetHeight());
            resized_ = wxBitmap(image_.Scale(std::max(1, int(image_.GetWidth() * scale)),
                                            std::max(1, int(image_.GetHeight() * scale)),
                                            wxIMAGE_QUALITY_NEAREST));
        }
        dc.DrawBitmap(resized_, (area.x - resized_.GetWidth()) / 2,
                       (area.y - resized_.GetHeight()) / 2);
        if (candidate && onPaint)
            onPaint(candidate->request, candidate->frame->pts);
    }

    wxImage image_;
    wxBitmap resized_;
    std::optional<Preview> pending_;
    std::shared_ptr<const VideoFrame> imageFrame_;
};

enum { openProject = wxID_HIGHEST + 1, exportMetrics, importCaptions, setProxy, measure };
constexpr int sliderRange = ScrubTimeline::range;

class Editor : public wxFrame {
public:
    Editor() : wxFrame(nullptr, wxID_ANY, "meditor", wxDefaultPosition, wxSize(1160, 710)), timer_(this)
    {
        using namespace view;
        SetBackgroundColour(canvas);
        useDarkTitleBar(*this);
        SetMinSize(FromDIP(wxSize(785, 590)));

        menu_ = std::make_unique<wxMenu>();
        menu_->Append(wxID_NEW, L"新規プロジェクト…\tCtrl+N");
        menu_->Append(openProject, L"プロジェクトを開く…\tCtrl+O");
        menu_->Append(wxID_SAVE, L"プロジェクトを保存\tCtrl+S");
        menu_->AppendSeparator();
        menu_->Append(setProxy, L"スクラブ用プロキシを選択…");
        menu_->Append(importCaptions, L"字幕を SRT から読み込む…");
        menu_->AppendCheckItem(measure, L"計測を開始・停止");
        menu_->Append(exportMetrics, L"計測を保存…");
        menu_->AppendSeparator();
        menu_->Append(wxID_EXIT, L"終了");
        const wxAcceleratorEntry shortcuts[] = {
            {wxACCEL_CTRL, 'N', wxID_NEW}, {wxACCEL_CTRL, 'O', openProject},
            {wxACCEL_CTRL, 'S', wxID_SAVE}, {wxACCEL_CTRL, 'Q', wxID_EXIT}
        };
        SetAcceleratorTable(wxAcceleratorTable(4, shortcuts));

        const int gap = FromDIP(12);
        const int padding = FromDIP(14);
        auto* root = new wxBoxSizer(wxVERTICAL);
        auto* header = panel(this, surface);
        auto* headerRow = new wxBoxSizer(wxHORIZONTAL);
        auto* brand = label(header, "meditor", 20, accent, true);
        projectName_ = label(header, L"プロジェクト未選択", 10, muted, false, wxST_ELLIPSIZE_END);
        auto* file = new CommandButton(header, L"ファイル", 76);
        auto* open = new CommandButton(header, L"開く", 66);
        auto* saveButton = new CommandButton(header, L"保存", 66);
        file->action = [this, header, file] {
            const auto position = file->GetPosition();
            header->PopupMenu(menu_.get(), position.x, position.y + file->GetSize().y);
        };
        open->action = [this] { openDialog(); };
        saveButton->action = [this] { save(); };
        headerRow->Add(brand, 0, wxALIGN_CENTER_VERTICAL | wxRIGHT, FromDIP(22));
        headerRow->Add(projectName_, 1, wxALIGN_CENTER_VERTICAL | wxRIGHT, gap);
        headerRow->Add(file, 0, wxRIGHT, FromDIP(6));
        headerRow->Add(open, 0, wxRIGHT, FromDIP(6));
        headerRow->Add(saveButton, 0);
        auto* headerInset = new wxBoxSizer(wxVERTICAL);
        headerInset->Add(headerRow, 0, wxEXPAND | wxALL, padding);
        header->SetSizer(headerInset);
        root->Add(header, 0, wxEXPAND);

        auto* split = new WorkspaceSplit(this);
        auto* left = panel(split, canvas);
        auto* right = panel(split, canvas);
        auto* previewCard = panel(left, surface);
        auto* captionCard = panel(right, surface);
        auto* previewLayout = new wxBoxSizer(wxVERTICAL);
        auto* previewHeader = new wxBoxSizer(wxHORIZONTAL);
        previewHeader->Add(label(previewCard, L"プレビュー", 12, text, true), 0, wxALIGN_CENTER_VERTICAL);
        previewHeader->AddStretchSpacer();
        proxy_ = new wxCheckBox(previewCard, wxID_ANY, L"プロキシ");
        proxy_->SetForegroundColour(muted);
        proxy_->SetBackgroundColour(surface);
        proxy_->SetToolTip(L"スクラブ中だけプロキシを使用");
        previewHeader->Add(proxy_, 0, wxALIGN_CENTER_VERTICAL);
        screen_ = new Screen(previewCard);
        previewLayout->Add(previewHeader, 0, wxEXPAND | wxLEFT | wxRIGHT | wxTOP, padding);
        previewLayout->Add(screen_, 1, wxEXPAND | wxALL, padding);
        previewCard->SetSizer(previewLayout);
        auto* leftLayout = new wxBoxSizer(wxVERTICAL);
        leftLayout->Add(previewCard, 1, wxEXPAND | wxRIGHT, FromDIP(4));
        left->SetSizer(leftLayout);

        auto* captionLayout = new wxBoxSizer(wxVERTICAL);
        auto* captionHeader = new wxBoxSizer(wxHORIZONTAL);
        captionHeader->Add(label(captionCard, L"字幕", 12, text, true), 0, wxALIGN_CENTER_VERTICAL);
        captionHeader->AddStretchSpacer();
        captionCount_ = label(captionCard, L"0 件", 9, muted);
        captionHeader->Add(captionCount_, 0, wxALIGN_CENTER_VERTICAL);
        captionLayout->Add(captionHeader, 0, wxEXPAND | wxBOTTOM, gap);
        auto* columns = panel(captionCard, field);
        auto* columnRow = new wxBoxSizer(wxHORIZONTAL);
        columnRow->AddSpacer(FromDIP(12));
        const wxString headings[]{L"時刻", L"話者", L"字幕テキスト"};
        for (std::size_t i = 0; i < 3; ++i) {
            auto* column = label(columns, headings[i], 9, muted, true);
            if (i < 2)
                column->SetMinSize(FromDIP(wxSize(i == 0 ? 88 : 79, -1)));
            columnRow->Add(column, 0, wxALIGN_CENTER_VERTICAL | wxTOP | wxBOTTOM, FromDIP(5));
        }
        columns->SetSizer(columnRow);
        captionLayout->Add(columns, 0, wxEXPAND | wxBOTTOM, FromDIP(3));
        list_ = new CaptionList(captionCard);
        captionLayout->Add(list_, 1, wxEXPAND | wxBOTTOM, gap);

        auto* speakerInput = paddedInput(captionCard, speaker_, 18);
        auto* captionInput = paddedInput(captionCard, text_, 42, wxTE_MULTILINE);
        captionLayout->Add(label(captionCard, L"話者", 9, muted, true), 0, wxBOTTOM, FromDIP(4));
        captionLayout->Add(speakerInput, 0, wxEXPAND | wxBOTTOM, FromDIP(7));
        captionLayout->Add(label(captionCard, L"字幕テキスト", 9, muted, true), 0, wxBOTTOM, FromDIP(4));
        captionLayout->Add(captionInput, 0, wxEXPAND);
        auto* captionInset = new wxBoxSizer(wxVERTICAL);
        captionInset->Add(captionLayout, 1, wxEXPAND | wxALL, padding);
        captionCard->SetSizer(captionInset);
        auto* rightLayout = new wxBoxSizer(wxVERTICAL);
        rightLayout->Add(captionCard, 1, wxEXPAND | wxLEFT, FromDIP(4));
        right->SetSizer(rightLayout);
        split->SetMinimumPaneSize(FromDIP(300));
        split->SetSashGravity(0.56);
        split->SplitVertically(left, right);
        CallAfter([split] { split->SetSashPosition(split->GetClientSize().x * 57 / 100); });
        root->Add(split, 1, wxEXPAND | wxLEFT | wxRIGHT | wxTOP, gap);

        auto* transport = panel(this, surface);
        auto* transportLayout = new wxBoxSizer(wxVERTICAL);
        auto* transportHeader = new wxBoxSizer(wxHORIZONTAL);
        auto* timelineTitle = label(transport, L"タイムライン", 11, text, true);
        play_ = new CommandButton(transport, L"再生", 96, true);
        time_ = label(transport, "00:00.000  /  00:00.000", 10, text, true);
        time_->SetFont(wxFont(wxFontInfo(10).Family(wxFONTFAMILY_TELETYPE).Bold()));
        transportHeader->Add(timelineTitle, 0, wxALIGN_CENTER_VERTICAL | wxRIGHT, padding);
        transportHeader->Add(time_, 0, wxALIGN_CENTER_VERTICAL);
        transportHeader->AddStretchSpacer();
        transportHeader->Add(play_, 0);
        timeline_ = new ScrubTimeline(transport);
        transportLayout->Add(transportHeader, 0, wxEXPAND | wxLEFT | wxRIGHT | wxTOP, padding);
        transportLayout->Add(timeline_, 0, wxEXPAND | wxLEFT | wxRIGHT | wxBOTTOM, FromDIP(7));
        transport->SetSizer(transportLayout);
        root->Add(transport, 0, wxEXPAND | wxLEFT | wxRIGHT | wxTOP, gap);
        status_ = label(this, "", 9, muted, false, wxST_ELLIPSIZE_END);
        root->Add(status_, 0, wxEXPAND | wxLEFT | wxRIGHT | wxTOP | wxBOTTOM, FromDIP(8));
        SetSizer(root);

        Bind(wxEVT_MENU, [this](wxCommandEvent&) { newProject(); }, wxID_NEW);
        Bind(wxEVT_MENU, [this](wxCommandEvent&) { importCaptionsDialog(); }, importCaptions);
        Bind(wxEVT_MENU, [this](wxCommandEvent&) { selectProxy(); }, setProxy);
        Bind(wxEVT_MENU, [this](wxCommandEvent&) { openDialog(); }, openProject);
        Bind(wxEVT_MENU, [this](wxCommandEvent&) { save(); }, wxID_SAVE);
        Bind(wxEVT_MENU, [this](wxCommandEvent&) { writeMetrics(); }, exportMetrics);
        Bind(wxEVT_MENU, [this](wxCommandEvent& event) {
            measuring_ = event.IsChecked();
            if (measuring_) {
                metrics_.clear();
                metricsDropped_ = 0;
                previousTick_ = {};
            }
        }, measure);
        Bind(wxEVT_MENU, [this](wxCommandEvent&) { Close(); }, wxID_EXIT);
        Bind(wxEVT_CLOSE_WINDOW, [this](wxCloseEvent& event) {
            if (event.CanVeto() && !mayLeave())
                event.Veto();
            else
                event.Skip();
        });
        Bind(wxEVT_TIMER, [this](wxTimerEvent&) { tick(); });
        screen_->accepts = [this](const VideoRequest& request) {
            return accepts(request);
        };
        screen_->onPaint = [this](VideoRequest request, Time pts) { observePaint(request, pts); };
        timeline_->track = [this](int value) { scrub(value); };
        timeline_->release = [this](int value) {
            if (scrubbing_)
                finishScrub(value);
        };
        timeline_->seek = [this](int value) {
            if (video_ && !scrubbing_ && value != shownSlider_)
                seekOriginal(fromSlider(value), playing_ || startWhenReady_);
        };
        play_->action = [this] { togglePlayback(); };
        list_->Bind(wxEVT_LISTBOX, [this](wxCommandEvent& e) { select(e.GetInt()); });
        list_->Bind(wxEVT_LISTBOX_DCLICK, [this](wxCommandEvent& e) {
            if (video_ && e.GetInt() >= 0)
                seekOriginal(project_.captions.at(e.GetInt()).start, false);
        });
        speaker_->Bind(wxEVT_TEXT, [this](wxCommandEvent&) { edit(); });
        text_->Bind(wxEVT_TEXT, [this](wxCommandEvent&) { edit(); });
        timeline_->Disable();
        proxy_->Disable();
        play_->Disable();
        select(-1);
        setStatusText(L"プロジェクトを開いてください");
        timer_.Start(8);
    }

    void closeAfterFirstFrame(wxString screenshot, std::function<void(bool)> done)
    {
        smokeScreenshot_ = std::move(screenshot);
        smokeDone_ = std::move(done);
    }

    bool load(const std::filesystem::path& path, bool interactive = true)
    {
        try {
            auto next = Project::load(path);
            auto player = std::make_unique<VideoWorker>(next.video, next.proxy);
            stopAudio();
            audio_.reset();
            video_ = std::move(player);
            project_ = std::move(next);
            path_ = path;
            dirty_ = false;
            scrubbing_ = false;
            pendingScrub_.reset();
            metrics_.clear();
            metricsDropped_ = 0;
            resetCaptions();
            if (!smokeScreenshot_.empty() && !project_.captions.empty()) {
                list_->SetSelection(0);
                select(0);
            }
            timeline_->setDuration(video_->duration());
            timeline_->Enable();
            proxy_->Enable(!project_.proxy.empty());
            proxy_->SetValue(!project_.proxy.empty());
            play_->Enable();
            screen_->clear();
            seekOriginal(0, false);
            const auto filename = display(meditor::utf8(path.filename()));
            SetTitle(L"meditor — " + filename);
            projectName_->SetLabel(filename);
            setStatusText(wxString::Format(L"音声 %zu トラック", project_.audio.size()));
            return true;
        } catch (const std::exception& error) {
            if (interactive)
                wxMessageBox(display(error.what()), L"プロジェクトを開けません", wxOK | wxICON_ERROR, this);
            else
                std::fprintf(stderr, "Could not load smoke-test project: %s\n", error.what());
            return false;
        }
    }

private:
    bool save()
    {
        if (path_.empty())
            return true;
        try {
            project_.save(path_);
            dirty_ = false;
            setStatusText(L"保存しました");
            return true;
        } catch (const std::exception& error) {
            wxMessageBox(display(error.what()), L"保存できません", wxOK | wxICON_ERROR, this);
            return false;
        }
    }

    bool mayLeave()
    {
        if (!dirty_)
            return true;
        const auto choice = wxMessageBox(L"字幕の変更を保存しますか？", "meditor",
                                        wxYES_NO | wxCANCEL | wxICON_QUESTION, this);
        return choice == wxNO || (choice == wxYES && save());
    }

    void newProject()
    {
        if (!mayLeave())
            return;
        wxFileDialog video(this, L"元動画を選択", {}, {}, L"動画|*.mp4;*.mkv;*.mov;*.webm|すべて|*",
                           wxFD_OPEN | wxFD_FILE_MUST_EXIST);
        if (video.ShowModal() != wxID_OK)
            return;
        wxFileDialog audio(this, L"ゲーム音・話者の WAV を選択（キャンセルで音声なし）", {}, {},
                           "Stereo 48 kHz PCM WAV|*.wav", wxFD_OPEN | wxFD_FILE_MUST_EXIST | wxFD_MULTIPLE);
        Project project;
        project.video = filePath(video.GetPath());
        if (audio.ShowModal() == wxID_OK) {
            wxArrayString paths;
            audio.GetPaths(paths);
            if (paths.size() > 32) {
                setStatusText(L"音声トラックは32本までです");
                return;
            }
            for (const auto& file : paths) {
                auto path = filePath(file);
                project.audio.push_back({path, meditor::utf8(path.stem()), 0, 1.0 / paths.size()});
            }
        }
        wxFileDialog destination(this, L"プロジェクトを保存", {}, "project.json", "meditor (*.json)|*.json",
                                 wxFD_SAVE | wxFD_OVERWRITE_PROMPT);
        if (destination.ShowModal() != wxID_OK)
            return;
        try {
            const auto path = filePath(destination.GetPath());
            project.save(path);
            load(path);
        } catch (const std::exception& error) {
            wxMessageBox(display(error.what()), L"プロジェクトを作成できません",
                         wxOK | wxICON_ERROR, this);
        }
    }

    void selectProxy()
    {
        if (!video_)
            return;
        wxFileDialog dialog(this, L"同じタイムラインのプロキシを選択", {}, {},
                            L"動画|*.mp4;*.mkv;*.mov;*.webm|すべて|*", wxFD_OPEN | wxFD_FILE_MUST_EXIST);
        if (dialog.ShowModal() != wxID_OK)
            return;
        try {
            auto proxy = filePath(dialog.GetPath());
            auto worker = std::make_unique<VideoWorker>(project_.video, proxy);
            const bool resume = playing_ || startWhenReady_;
            stopAudio();
            scrubbing_ = false;
            pendingScrub_.reset();
            project_.proxy = std::move(proxy);
            video_ = std::move(worker);
            proxy_->Enable();
            proxy_->SetValue(true);
            dirty_ = true;
            seekOriginal(position_, resume);
        } catch (const std::exception& error) {
            wxMessageBox(display(error.what()), L"プロキシを設定できません", wxOK | wxICON_ERROR, this);
        }
    }

    void importCaptionsDialog()
    {
        if (!video_)
            return;
        if (!project_.captions.empty() &&
            wxMessageBox(L"現在の字幕を置き換えますか？", "meditor",
                         wxYES_NO | wxNO_DEFAULT | wxICON_QUESTION, this) != wxYES)
            return;
        wxFileDialog dialog(this, L"UTF-8 SRT を選択", {}, {}, "SubRip (*.srt)|*.srt",
                            wxFD_OPEN | wxFD_FILE_MUST_EXIST);
        if (dialog.ShowModal() != wxID_OK)
            return;
        try {
            project_.captions = importSrt(filePath(dialog.GetPath()));
            resetCaptions();
            dirty_ = true;
        } catch (const std::exception& error) {
            wxMessageBox(display(error.what()), L"字幕を読み込めません",
                         wxOK | wxICON_ERROR, this);
        }
    }

    void openDialog()
    {
        if (!mayLeave())
            return;
        wxFileDialog dialog(this, L"プロジェクトを開く", {}, {}, "meditor (*.json)|*.json",
                            wxFD_OPEN | wxFD_FILE_MUST_EXIST);
        if (dialog.ShowModal() == wxID_OK)
            load(filePath(dialog.GetPath()));
    }

    void setStatusText(const wxString& message)
    {
        status_->SetLabel(message);
    }

    void resetCaptions()
    {
        list_->reset(project_);
        captionCount_->SetLabel(wxString::Format(L"%zu 件", project_.captions.size()));
        captionCount_->GetParent()->Layout();
        select(-1);
    }

    void select(long row)
    {
        selected_ = row;
        const bool enabled = row >= 0 && static_cast<std::size_t>(row) < project_.captions.size();
        speaker_->Enable(enabled);
        text_->Enable(enabled);
        speaker_->ChangeValue(enabled ? display(project_.captions[row].speaker) : wxString{});
        text_->ChangeValue(enabled ? display(project_.captions[row].text) : wxString{});
    }

    void edit()
    {
        if (selected_ < 0 || static_cast<std::size_t>(selected_) >= project_.captions.size())
            return;
        const auto began = Clock::now();
        auto& caption = project_.captions[selected_];
        caption.speaker = utf8(speaker_->GetValue());
        caption.text = utf8(text_->GetValue());
        dirty_ = true;
        list_->RefreshRow(selected_);
        if (measuring_)
            record({{"event", "caption_edit_dispatch"}, {"us", elapsed(began)}});
    }

    Time fromSlider(int value) const
    {
        return (static_cast<Time>(value) * video_->duration()) / sliderRange;
    }

    void showPosition(Time position)
    {
        position_ = std::clamp(position, Time{0}, video_->duration());
        const auto slider = static_cast<int>((position_ * sliderRange + video_->duration() - 1) / video_->duration());
        if (!scrubbing_) {
            shownSlider_ = slider;
            timeline_->setValue(slider);
        }
        time_->SetLabel(view::timeText(position_) + "  /  " + view::timeText(video_->duration()));
    }

    void stopAudio()
    {
        if (audio_)
            audio_->stop();
        playing_ = false;
        startWhenReady_ = false;
        if (play_)
            play_->SetLabel(L"再生");
    }

    void scrub(int value)
    {
        if (!video_)
            return;
        if (!scrubbing_) {
            resumeAfterScrub_ = playing_ || startWhenReady_;
            stopAudio();
            scrubbing_ = true;
            ++epoch_;
        }
        showPosition(fromSlider(value));
        pendingScrub_ = VideoRequest{position_, epoch_, proxy_->IsEnabled() && proxy_->GetValue(),
                                    true, Clock::now()};
        latestScrubAt_ = pendingScrub_->issued;
        if (measuring_)
            record({{"event", "scrub_input"}, {"target_us", position_}, {"proxy", pendingScrub_->proxy}});
    }

    void finishScrub(int value)
    {
        scrubbing_ = false;
        pendingScrub_.reset();
        seekOriginal(fromSlider(value), resumeAfterScrub_);
    }

    void seekOriginal(Time position, bool resume)
    {
        stopAudio();
        showPosition(position);
        startWhenReady_ = resume;
        play_->SetLabel(resume ? L"停止" : L"再生");
        video_->submit({position_, ++epoch_, false, false, Clock::now()});
    }

    void togglePlayback()
    {
        if (!video_ || scrubbing_)
            return;
        if (playing_ || startWhenReady_) {
            seekOriginal(position_, false);
        } else {
            seekOriginal(position_ == video_->duration() ? 0 : position_, true);
        }
    }

    void tick()
    {
        if (!video_)
            return;
        try {
            const auto now = Clock::now();
            if (smokeCaptureAt_ && now >= *smokeCaptureAt_) {
                smokeCaptureAt_.reset();
                finishSmokeTest();
                return;
            }
            if (measuring_) {
                if (previousTick_ != Clock::time_point{})
                    record({{"event", "gui_timer_interval"}, {"us", elapsed(previousTick_)}});
                previousTick_ = now;
            }
            if (pendingScrub_ && now - lastScrubSend_ >= std::chrono::milliseconds(32)) {
                lastScrubSend_ = now;
                video_->submit(*std::exchange(pendingScrub_, std::nullopt));
            }
            if (auto result = video_->take(); result && accepts(result->request)) {
                if (!result->error.empty()) {
                    setStatusText(display(result->error));
                    if (smokeDone_) {
                        std::fprintf(stderr, "Could not decode smoke-test frame: %s\n", result->error.c_str());
                        finishSmokeTest(false);
                        return;
                    }
                    if (result->request.proxy)
                        proxy_->SetValue(false);
                    if (startWhenReady_)
                        stopAudio();
                } else {
                    screen_->showFrame(*result);
                    if (startWhenReady_) {
                        if (!audio_)
                            audio_ = std::make_unique<AudioMixer>(project_.audio, video_->duration());
                        audio_->start(position_);
                        startWhenReady_ = false;
                        playing_ = true;
                        play_->SetLabel(L"一時停止");
                        if (measuring_) {
                            resumePaint_ = result->request.issued;
                            record({{"event", "resume_queued"}, {"position_us", position_},
                                    {"video_pts_us", result->frame->pts}});
                        }
                    }
                }
            }
            if (playing_) {
                showPosition(audio_->position());
                if (position_ >= video_->duration()) {
                    stopAudio();
                } else if (now - lastPlaySend_ >= std::chrono::milliseconds(16)) {
                    lastPlaySend_ = now;
                    video_->submit({position_, epoch_, false, false, now});
                }
            }
        } catch (const std::exception& error) {
            stopAudio();
            audio_.reset();
            setStatusText(display(error.what()));
        }
    }

    static Time elapsed(Clock::time_point began)
    {
        return std::chrono::duration_cast<std::chrono::microseconds>(Clock::now() - began).count();
    }

    bool accepts(const VideoRequest& request) const
    {
        return request.epoch == epoch_ && (!request.scrub || request.issued == latestScrubAt_);
    }

    void finishSmokeTest(bool success = true)
    {
        if (success && !smokeScreenshot_.empty()) {
#ifdef __WXMSW__
            success = saveWindowsScreenshot(*this, smokeScreenshot_);
#else
            success = false;
#endif
            if (!success)
                std::fputs("Could not capture the editor with its synthetic preview.\n", stderr);
        }
        if (auto done = std::exchange(smokeDone_, {})) {
            done(success);
            Close();
        }
    }

    void observePaint(const VideoRequest& request, Time pts)
    {
        if (smokeDone_ && !smokeCaptureAt_) {
            if (smokeScreenshot_.empty()) {
                CallAfter([this] { finishSmokeTest(); });
            } else {
                // Allow native child controls and the buffered preview to finish painting.
                smokeCaptureAt_ = Clock::now() + std::chrono::milliseconds(250);
            }
        }
        if (!measuring_)
            return;
        if (request.scrub)
            record({{"event", "scrub_paint"}, {"us", elapsed(request.issued)},
                    {"target_us", request.target}, {"video_pts_us", pts}, {"proxy", request.proxy}});
        if (request.issued == resumePaint_ && playing_) {
            try {
                record({{"event", "resume_paint"}, {"video_pts_us", pts},
                        {"audio_queue_position_us", audio_->position()}});
            } catch (const std::exception&) {
                // The transport timer reports audio failures.
            }
        }
    }

    void record(nlohmann::json event)
    {
        if (metrics_.size() < 100'000)
            metrics_.push_back(std::move(event));
        else
            ++metricsDropped_;
    }

    void writeMetrics()
    {
        measuring_ = false;
        menu_->Check(measure, false);
        wxFileDialog dialog(this, L"計測を保存", {}, "preview-metrics.json",
                            "JSON (*.json)|*.json", wxFD_SAVE | wxFD_OVERWRITE_PROMPT);
        if (dialog.ShowModal() != wxID_OK)
            return;
        std::ofstream stream(filePath(dialog.GetPath()), std::ios::binary);
        stream << nlohmann::json{{"version", 1}, {"truncated_events", metricsDropped_},
                                {"events", metrics_}}.dump(2) << '\n';
        if (!stream)
            setStatusText(L"計測ファイルを書き込めません");
    }

    Project project_;
    std::filesystem::path path_;
    std::unique_ptr<VideoWorker> video_;
    std::unique_ptr<AudioMixer> audio_;
    Screen* screen_ = nullptr;
    CaptionList* list_ = nullptr;
    ScrubTimeline* timeline_ = nullptr;
    wxCheckBox* proxy_ = nullptr;
    CommandButton* play_ = nullptr;
    wxStaticText* time_ = nullptr;
    wxStaticText* projectName_ = nullptr;
    wxStaticText* captionCount_ = nullptr;
    wxStaticText* status_ = nullptr;
    std::unique_ptr<wxMenu> menu_;
    wxTextCtrl* speaker_ = nullptr;
    wxTextCtrl* text_ = nullptr;
    wxTimer timer_;
    long selected_ = -1;
    bool dirty_ = false;
    bool playing_ = false;
    bool startWhenReady_ = false;
    bool scrubbing_ = false;
    bool resumeAfterScrub_ = false;
    int shownSlider_ = 0;
    Time position_ = 0;
    std::uint64_t epoch_ = 0;
    std::optional<VideoRequest> pendingScrub_;
    Clock::time_point lastScrubSend_{};
    Clock::time_point lastPlaySend_{};
    Clock::time_point latestScrubAt_{};
    Clock::time_point previousTick_{};
    Clock::time_point resumePaint_{};
    bool measuring_ = false;
    std::optional<Clock::time_point> smokeCaptureAt_;
    wxString smokeScreenshot_;
    std::function<void(bool)> smokeDone_;
    std::size_t metricsDropped_ = 0;
    std::vector<nlohmann::json> metrics_;
};

class App : public wxApp {
public:
    bool OnInit() override
    {
        auto* editor = new Editor;
        const bool smoke = argc > 1 && argv[1] == "--smoke-test";
        if (smoke && argc != 3 && argc != 4) {
            editor->Destroy();
            return false;
        }
        if (smoke) {
            if (argc == 4) {
                editor->SetSize(960, 680);
                editor->Move(0, 0);
            }
            editor->closeAfterFirstFrame(argc == 4 ? wxString(argv[3]) : wxString{},
                [this](bool success) { smokeFailed_ = !success; });
            if (!editor->load(filePath(argv[2]), false)) {
                editor->Destroy();
                return false;
            }
        }
        editor->Show();
        if (!smoke && argc > 1)
            editor->load(filePath(argv[1]));
        return true;
    }

    int OnRun() override
    {
        const auto result = wxApp::OnRun();
        return smokeFailed_ ? 1 : result;
    }

private:
    bool smokeFailed_ = false;
};
}
}
wxIMPLEMENT_APP(meditor::App);
