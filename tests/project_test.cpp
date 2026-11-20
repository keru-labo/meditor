#include "project.h"

#include <chrono>
#include <fstream>
#include <iostream>
#include <stdexcept>

using namespace meditor;

static void expect(bool condition, const char* scenario)
{
    if (!condition)
        throw std::runtime_error(scenario);
}

int main()
{
    namespace fs = std::filesystem;
    const auto directory = fs::temp_directory_path() /
        ("meditor-test-" + std::to_string(std::chrono::steady_clock::now().time_since_epoch().count()));
    fs::create_directory(directory);
    try {
        const auto file = directory / "project.json";
        Project project;
        project.video = directory / "recording.mkv";
        project.proxy = directory / "proxy.mp4";
        project.audio = {{directory / "game.wav", "ゲーム音", 0, 0.5},
                         {directory / "voice.wav", "話者1", -22'000, 1.0}};
        for (int i = 9999; i >= 0; --i)
            project.captions.push_back({std::to_string(i), i * 100'000LL, i * 100'000LL + 50'000,
                                        "話者1", "実況字幕"});
        project.save(file);
        auto loaded = Project::load(file);
        expect(loaded.captions.size() == 10'000, "load all captions");
        expect(loaded.captions[0].id == "0" && loaded.captions.back().id == "9999",
               "sort timeline without changing stable IDs");
        expect(loaded.video == project.video && loaded.proxy == project.proxy, "resolve relative media");
        expect(loaded.audio[1].offset == -22'000 && loaded.audio[0].gain == 0.5, "preserve audio offsets and gain");
        loaded.captions[5000].text = "変更した字幕\n次の行";
        loaded.save(file);
        expect(Project::load(file).captions[5000].text == loaded.captions[5000].text,
               "round-trip Japanese edit and replace existing project");
        expect(!loaded.clip && loaded.playbackRange(90 * second) == ClipRange{0, 90 * second},
               "Legacy version 1 projects use the entire source");
        const ClipRange chosen{30 * second, 40 * second};
        expect(loaded.setClip(chosen, 90 * second) && loaded.clip == chosen,
               "Apply one valid source interval");
        expect(loaded.playbackRange(90 * second).duration() == 10 * second,
               "The clip starts at timeline zero and determines timeline duration");

        const ClipRange trimmed{32 * second, 40 * second};
        expect(loaded.setClip(trimmed, 90 * second), "Trim the beginning of the applied clip");
        for (const ClipRange invalid : {ClipRange{-second, 40 * second},
                                        ClipRange{32 * second, 91 * second},
                                        ClipRange{40 * second, 40 * second},
                                        ClipRange{32 * second, 31 * second}}) {
            expect(!loaded.setClip(invalid, 90 * second) && loaded.clip == trimmed,
                   "Invalid trim retains the previously applied range");
        }
        loaded.save(file);
        const auto reopened = Project::load(file);
        expect(reopened.clip == trimmed && reopened.playbackRange(90 * second).duration() == 8 * second,
               "Reopen the edited clip");
        expect(reopened.audio[1].offset == loaded.audio[1].offset &&
               reopened.captions[5000].text == loaded.captions[5000].text,
               "Saving a clip retains existing audio settings and subtitle edits");

        loaded.captions[2].id = loaded.captions[1].id;
        loaded.save(file);
        bool rejected = false;
        try {
            Project::load(file);
        } catch (const std::runtime_error&) {
            rejected = true;
        }
        expect(rejected, "reject duplicate IDs");
        const auto srt = directory / "captions.srt";
        {
            std::ofstream stream(srt, std::ios::binary);
            stream << "\xef\xbb\xbf" "1\r\n00:00:03,500 --> 00:00:04,100\r\n"
                      "実況字幕\r\n次の行\r\n\r\n"
                      "2\r\n00:00:00,000 --> 00:00:01,200\r\n先頭";
        }
        const auto imported = importSrt(srt);
        expect(imported.size() == 2 && imported[0].start == 0 && imported[1].start == 3'500'000 &&
               imported[1].text == "実況字幕\n次の行", "import Japanese SRT with BOM and CRLF");
    } catch (const std::exception& error) {
        fs::remove_all(directory);
        std::cerr << error.what() << '\n';
        return 1;
    }
    fs::remove_all(directory);
}
