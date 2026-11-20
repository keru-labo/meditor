#include "project.h"

#include <iostream>

int main(int argc, char** argv)
{
    using namespace meditor;
    if (argc != 2) {
        std::cerr << "Usage: meditor_fixture OUTPUT_DIRECTORY\n";
        return 1;
    }
    try {
        const auto directory = std::filesystem::absolute(fromUtf8(argv[1]));
        Project project;
        project.video = directory / "original.mp4";
        project.proxy = directory / "proxy.mp4";
        project.audio = {{directory / "game.wav", "ゲーム音", 0, 0.30},
                         {directory / "voice-a.wav", "話者 A", 0, 0.55},
                         {directory / "voice-b.wav", "話者 B", 120'000, 0.55}};
        for (int i = 0; i < 10'000; ++i) {
            const Time start = i * 6'000LL;
            project.captions.push_back({std::to_string(i + 1), start, start + 350'000,
                i % 2 ? "話者 A" : "話者 B", "実況字幕 " + std::to_string(i + 1)});
        }
        project.save(directory / "fixture.json");
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
