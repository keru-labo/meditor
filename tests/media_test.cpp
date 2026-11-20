#include "audio.h"
#include "video.h"

#include <algorithm>
#include <iostream>
#include <stdexcept>
#include <thread>

using namespace meditor;
using namespace std::chrono_literals;

static Preview waitFor(VideoWorker& worker)
{
    const auto deadline = Clock::now() + 5s;
    while (Clock::now() < deadline) {
        if (auto result = worker.take()) {
            if (!result->frame)
                throw std::runtime_error(result->error);
            return std::move(*result);
        }
        std::this_thread::sleep_for(2ms);
    }
    throw std::runtime_error("Timed out waiting for frame");
}

static void expect(bool condition, const char* scenario)
{
    if (!condition)
        throw std::runtime_error(scenario);
}

int main(int argc, char** argv)
{
    if (argc != 2)
        return 1;
    try {
        const auto project = Project::load(fromUtf8(argv[1]));
        expect(project.captions.size() == 10'000 && project.audio.size() == 3, "Fixture contents");
        VideoWorker worker(project.video, project.proxy);
        expect(worker.duration() == 60 * second, "Original duration");
        worker.submit({3 * second, 1, false, true, Clock::now()});
        worker.submit({43 * second, 1, false, true, Clock::now()});
        worker.submit({7 * second, 1, false, true, Clock::now()});
        const auto original = waitFor(worker);
        expect(original.request.target == 7 * second && original.frame->pts == 7 * second,
               "Latest original seek wins");

        worker.submit({13 * second, 2, true, true, Clock::now()});
        const auto proxy = waitFor(worker);
        expect(proxy.request.proxy && proxy.frame->pts == 13 * second &&
               proxy.frame->width < original.frame->width, "Proxy uses the same project timeline");

        worker.submit({13 * second, 3, false, false, Clock::now()});
        const auto resumed = waitFor(worker);
        expect(!resumed.request.proxy && resumed.frame->pts == proxy.frame->pts,
               "Resume from the same original timestamp");
        worker.submit({60 * second, 4, false, false, Clock::now()});
        const auto last = waitFor(worker);
        expect(last.frame->pts >= 59'950'000 && last.frame->pts < 60 * second, "Real last frame PTS");

        AudioMixer silent({}, worker.duration());
        silent.start(13 * second);
        std::this_thread::sleep_for(2ms);
        expect(silent.position() > 13 * second, "Video-only playback has a clock without an audio device");

        AudioMixer mix(project.audio, worker.duration());
        mix.start(13 * second);
        const auto deadline = Clock::now() + 1s;
        while (Clock::now() < deadline && mix.position() <= 13 * second) {
            std::this_thread::sleep_for(5ms);
        }
        expect(mix.position() > 13 * second && mix.position() < 14 * second, "Audio clock resumes in range");
        mix.stop();
        mix.start(5 * second);
        expect(mix.position() >= 5 * second && mix.position() < 6 * second, "Audio seek clears old queue");
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
