#pragma once

#include "project.h"

#include <chrono>
#include <memory>
#include <optional>

namespace meditor {
using Clock = std::chrono::steady_clock;

struct VideoRequest {
    Time target;
    std::uint64_t epoch;
    bool proxy;
    bool scrub;
    Clock::time_point issued;
};

struct VideoFrame {
    Time pts;
    int width;
    int height;
    int stride;
    std::vector<unsigned char> rgb;
};

struct Preview {
    VideoRequest request;
    std::shared_ptr<const VideoFrame> frame;
    std::string error;
};

class VideoWorker {
public:
    VideoWorker(const std::filesystem::path& original, const std::filesystem::path& proxy);
    ~VideoWorker();
    Time duration() const;
    void submit(VideoRequest request);
    std::optional<Preview> take();

private:
    struct State;
    std::unique_ptr<State> state_;
};
}
