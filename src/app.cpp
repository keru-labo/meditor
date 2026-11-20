#include "audio.h"
#include "project.h"
#include "video.h"

#include <wx/wx.h>
#include <wx/dcbuffer.h>
#include <wx/listctrl.h>
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

wxString timeText(Time time)
{
    return wxString::Format("%02lld:%02lld.%03lld",
        static_cast<long long>(time / (60 * second)),
        static_cast<long long>(time / second % 60),
        static_cast<long long>(time / 1000 % 1000));
}

class CaptionList : public wxListCtrl {
public:
    explicit CaptionList(wxWindow* parent)
        : wxListCtrl(parent, wxID_ANY, wxDefaultPosition, wxDefaultSize,
                    wxLC_REPORT | wxLC_VIRTUAL | wxLC_SINGLE_SEL)
    {
        AppendColumn(L"時刻", wxLIST_FORMAT_LEFT, 98);
        AppendColumn(L"話者", wxLIST_FORMAT_LEFT, 90);
        AppendColumn(L"字幕", wxLIST_FORMAT_LEFT, 300);
    }

    void reset(const Project& project)
    {
        const long selected = GetNextItem(-1, wxLIST_NEXT_ALL, wxLIST_STATE_SELECTED);
        if (selected >= 0)
            SetItemState(selected, 0, wxLIST_STATE_SELECTED | wxLIST_STATE_FOCUSED);
        project_ = &project;
        SetItemCount(static_cast<long>(project.captions.size()));
        Refresh();
    }

private:
    wxString OnGetItemText(long row, long column) const override
    {
        if (!project_ || row < 0 || static_cast<std::size_t>(row) >= project_->captions.size())
            return {};
        const auto& caption = project_->captions[row];
        if (column == 0)
            return timeText(caption.start);
        if (column == 1)
            return display(caption.speaker);
        return display(caption.text);
    }

    const Project* project_ = nullptr;
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
        dc.SetBackground(wxBrush(wxColour(22, 25, 30)));
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
        if (!image_.IsOk() || area.x < 1 || area.y < 1)
            return;
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
constexpr int sliderRange = 1'000'000;

class Editor : public wxFrame {
public:
    Editor() : wxFrame(nullptr, wxID_ANY, "meditor", wxDefaultPosition, wxSize(1160, 710)), timer_(this)
    {
        auto* menu = new wxMenu;
        menu->Append(wxID_NEW, L"新規プロジェクト…\tCtrl+N");
        menu->Append(openProject, L"プロジェクトを開く…\tCtrl+O");
        menu->Append(setProxy, L"スクラブ用プロキシを選択…");
        menu->Append(importCaptions, L"字幕を SRT から読み込む…");
        menu->Append(wxID_SAVE, L"プロジェクトを保存\tCtrl+S");
        menu->AppendCheckItem(measure, L"計測を開始・停止");
        menu->Append(exportMetrics, L"計測を保存…");
        menu->AppendSeparator();
        menu->Append(wxID_EXIT, L"終了");
        auto* bar = new wxMenuBar;
        bar->Append(menu, L"ファイル");
        SetMenuBar(bar);
        CreateStatusBar();
        SetMinSize(wxSize(760, 480));

        auto* split = new wxSplitterWindow(this);
        auto* left = new wxPanel(split);
        auto* right = new wxPanel(split);
        auto* player = new wxBoxSizer(wxVERTICAL);
        auto* controls = new wxBoxSizer(wxHORIZONTAL);
        auto* captions = new wxBoxSizer(wxVERTICAL);
        screen_ = new Screen(left);
        timeline_ = new wxSlider(left, wxID_ANY, 0, 0, sliderRange);
        timeline_->SetName(L"タイムライン");
        play_ = new wxButton(left, wxID_ANY, L"再生");
        proxy_ = new wxCheckBox(left, wxID_ANY, L"スクラブにプロキシを使う");
        time_ = new wxStaticText(left, wxID_ANY, "00:00.000");
        controls->Add(play_, 0, wxRIGHT, 12);
        controls->Add(proxy_, 0, wxALIGN_CENTER_VERTICAL | wxRIGHT, 12);
        controls->Add(time_, 0, wxALIGN_CENTER_VERTICAL);
        player->Add(screen_, 1, wxEXPAND);
        player->Add(timeline_, 0, wxEXPAND | wxTOP, 8);
        player->Add(controls, 0, wxEXPAND | wxTOP, 8);
        auto* leftMargin = new wxBoxSizer(wxVERTICAL);
        leftMargin->Add(player, 1, wxEXPAND | wxALL, 12);
        left->SetSizer(leftMargin);

        list_ = new CaptionList(right);
        speaker_ = new wxTextCtrl(right, wxID_ANY);
        text_ = new wxTextCtrl(right, wxID_ANY, {}, wxDefaultPosition, wxSize(-1, 90), wxTE_MULTILINE);
        captions->Add(list_, 1, wxEXPAND | wxBOTTOM, 12);
        captions->Add(new wxStaticText(right, wxID_ANY, L"話者"), 0, wxBOTTOM, 4);
        captions->Add(speaker_, 0, wxEXPAND | wxBOTTOM, 8);
        captions->Add(new wxStaticText(right, wxID_ANY, L"字幕"), 0, wxBOTTOM, 4);
        captions->Add(text_, 0, wxEXPAND);
        auto* rightMargin = new wxBoxSizer(wxVERTICAL);
        rightMargin->Add(captions, 1, wxEXPAND | wxALL, 12);
        right->SetSizer(rightMargin);
        split->SetMinimumPaneSize(260);
        split->SetSashGravity(0.55);
        split->SplitVertically(left, right, 620);
        CallAfter([split] { split->SetSashPosition(split->GetClientSize().x * 56 / 100); });
        auto* layout = new wxBoxSizer(wxVERTICAL);
        layout->Add(split, 1, wxEXPAND);
        SetSizer(layout);

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
        timeline_->Bind(wxEVT_SCROLL_THUMBTRACK, [this](wxScrollEvent& e) { scrub(e.GetPosition()); });
        timeline_->Bind(wxEVT_SCROLL_THUMBRELEASE, [this](wxScrollEvent& e) {
            if (scrubbing_)
                finishScrub(e.GetPosition());
        });
        timeline_->Bind(wxEVT_SLIDER, [this](wxCommandEvent& e) {
            if (video_ && !scrubbing_ && e.GetInt() != shownSlider_)
                seekOriginal(fromSlider(e.GetInt()), playing_ || startWhenReady_);
        });
        play_->Bind(wxEVT_BUTTON, [this](wxCommandEvent&) { togglePlayback(); });
        list_->Bind(wxEVT_LIST_ITEM_SELECTED, [this](wxListEvent& e) { select(e.GetIndex()); });
        list_->Bind(wxEVT_LIST_ITEM_ACTIVATED, [this](wxListEvent& e) {
            if (video_ && e.GetIndex() >= 0)
                seekOriginal(project_.captions.at(e.GetIndex()).start, false);
        });
        speaker_->Bind(wxEVT_TEXT, [this](wxCommandEvent&) { edit(); });
        text_->Bind(wxEVT_TEXT, [this](wxCommandEvent&) { edit(); });
        timeline_->Disable();
        proxy_->Disable();
        play_->Disable();
        select(-1);
        SetStatusText(L"プロジェクトを開いてください");
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
            list_->reset(project_);
            select(-1);
            timeline_->Enable();
            proxy_->Enable(!project_.proxy.empty());
            proxy_->SetValue(!project_.proxy.empty());
            play_->Enable();
            screen_->clear();
            seekOriginal(0, false);
            SetTitle(L"meditor — " + display(meditor::utf8(path.filename())));
            SetStatusText(wxString::Format(L"%zu 件の字幕 / %zu 音声トラック",
                                           project_.captions.size(), project_.audio.size()));
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
            SetStatusText(L"保存しました");
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
                SetStatusText(L"音声トラックは32本までです");
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
            list_->reset(project_);
            select(-1);
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
        list_->RefreshItem(selected_);
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
            timeline_->SetValue(slider);
        }
        time_->SetLabel(timeText(position_));
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
                    SetStatusText(display(result->error));
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
                        play_->SetLabel(L"ポーズ");
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
            SetStatusText(display(error.what()));
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
        GetMenuBar()->Check(measure, false);
        wxFileDialog dialog(this, L"計測を保存", {}, "preview-metrics.json",
                            "JSON (*.json)|*.json", wxFD_SAVE | wxFD_OVERWRITE_PROMPT);
        if (dialog.ShowModal() != wxID_OK)
            return;
        std::ofstream stream(filePath(dialog.GetPath()), std::ios::binary);
        stream << nlohmann::json{{"version", 1}, {"truncated_events", metricsDropped_},
                                {"events", metrics_}}.dump(2) << '\n';
        if (!stream)
            SetStatusText(L"計測ファイルを書き込めません");
    }

    Project project_;
    std::filesystem::path path_;
    std::unique_ptr<VideoWorker> video_;
    std::unique_ptr<AudioMixer> audio_;
    Screen* screen_ = nullptr;
    CaptionList* list_ = nullptr;
    wxSlider* timeline_ = nullptr;
    wxCheckBox* proxy_ = nullptr;
    wxButton* play_ = nullptr;
    wxStaticText* time_ = nullptr;
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
