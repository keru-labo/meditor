#pragma once

#include "project.h"

#include <memory>

namespace meditor {
class AudioMixer {
public:
    AudioMixer(const std::vector<Track>& tracks, Time duration);
    ~AudioMixer();
    void start(Time position);
    void stop();
    Time position() const;

private:
    struct State;
    std::unique_ptr<State> state_;
};
}
