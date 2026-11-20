#include "audio.h"

#define SDL_MAIN_HANDLED
#include <SDL.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <condition_variable>
#include <mutex>
#include <thread>
#include <filesystem>
#include <fstream>
#include <stdexcept>
#include <string_view>
#include <utility>

namespace meditor {
namespace {
constexpr int sampleRate = 48000;
constexpr int channels = 2;
constexpr int blockFrames = 1024;
constexpr int frameBytes = channels * sizeof(std::int16_t);

Time toFrames(Time time) { return time * sampleRate / second; }
Time toTime(Time frames) { return frames * second / sampleRate; }

unsigned le16(const unsigned char* bytes)
{
    return unsigned(bytes[0]) | (unsigned(bytes[1]) << 8);
}

std::uint32_t le32(const unsigned char* bytes)
{
    return le16(bytes) | (std::uint32_t(le16(bytes + 2)) << 16);
}

struct Wave {
    std::ifstream input;
    std::streamoff data = 0;
    Time frames = 0;
    Time offset = 0;
    double gain;

    explicit Wave(const Track& track)
        : input(track.path, std::ios::binary), offset(toFrames(track.offset)), gain(track.gain)
    {
        if (!input)
            throw std::runtime_error("Cannot open audio track: " + track.name);
        std::array<unsigned char, 16> bytes{};
        auto id = [&] { return std::string_view(reinterpret_cast<char*>(bytes.data()), 4); };
        auto read = [&](int size) {
            return bool(input.read(reinterpret_cast<char*>(bytes.data()), size));
        };
        if (!read(8) || id() != "RIFF" || !read(4) || id() != "WAVE")
            throw std::runtime_error("Expected RIFF/WAVE: " + track.name);

        const auto limit = std::filesystem::file_size(track.path);
        bool format = false;
        while (read(8)) {
            const auto size = le32(bytes.data() + 4);
            const auto start = static_cast<std::streamoff>(input.tellg());
            const auto next = start + size + (size & 1u);
            if (next < 0 || static_cast<std::uintmax_t>(next) > limit)
                break;
            if (id() == "fmt " && size >= 16 && read(16)) {
                format = le16(bytes.data()) == 1 && le16(bytes.data() + 2) == channels &&
                         le32(bytes.data() + 4) == sampleRate && le16(bytes.data() + 14) == 16 &&
                         le16(bytes.data() + 12) == frameBytes;
            } else if (id() == "data") {
                data = start;
                frames = size / frameBytes;
            }
            if (format && data != 0)
                return;
            input.seekg(next);
        }
        throw std::runtime_error("Convert audio to stereo 48 kHz PCM s16le: " + track.name);
    }

    void mix(Time first, int count, std::array<float, blockFrames * channels>& mixed)
    {
        const auto source = first - offset;
        const auto begin = std::max(Time{0}, source);
        const auto end = std::min(frames, source + count);
        if (begin >= end || gain == 0)
            return;
        std::array<unsigned char, blockFrames * frameBytes> bytes{};
        input.clear();
        input.seekg(data + static_cast<std::streamoff>(begin * frameBytes));
        if (!input.read(reinterpret_cast<char*>(bytes.data()), (end - begin) * frameBytes))
            throw std::runtime_error("Could not read audio samples");
        const auto output = (begin - source) * channels;
        for (Time i = 0; i < (end - begin) * channels; ++i) {
            const auto sample = le16(bytes.data() + i * 2);
            const auto signedSample = sample >= 32768 ? static_cast<int>(sample) - 65536 : static_cast<int>(sample);
            mixed[output + i] += static_cast<float>(signedSample * gain);
        }
    }
};
}

struct AudioMixer::State {
    std::vector<Wave> tracks;
    SDL_AudioDeviceID device = 0;
    const Time duration;
    const Time end;
    std::mutex mutex;
    std::condition_variable wake;
    std::uint64_t epoch = 0;
    Time written = 0;
    Time silentPosition = 0;
    std::chrono::steady_clock::time_point silentStart;
    bool active = false;
    bool preroll = false;
    std::string error;
    std::jthread worker;

    State(const std::vector<Track>& source, Time duration)
        : duration(duration), end(toFrames(duration))
    {
        tracks.reserve(source.size());
        for (const auto& track : source)
            tracks.emplace_back(track);
        if (tracks.empty())
            return;
        SDL_SetMainReady();
        if (SDL_InitSubSystem(SDL_INIT_AUDIO) != 0)
            throw std::runtime_error(SDL_GetError());

        SDL_AudioSpec desired{};
        desired.freq = sampleRate;
        desired.format = AUDIO_S16SYS;
        desired.channels = channels;
        desired.samples = blockFrames;
        device = SDL_OpenAudioDevice(nullptr, 0, &desired, nullptr, 0);
        if (!device) {
            const std::string error = SDL_GetError();
            SDL_QuitSubSystem(SDL_INIT_AUDIO);
            throw std::runtime_error(error);
        }
        try {
            worker = std::jthread([this](std::stop_token stop) { run(stop); });
        } catch (...) {
            SDL_CloseAudioDevice(device);
            SDL_QuitSubSystem(SDL_INIT_AUDIO);
            throw;
        }
    }

    ~State()
    {
        if (!device)
            return;
        worker.request_stop();
        wake.notify_all();
        worker.join();
        SDL_CloseAudioDevice(device);
        SDL_QuitSubSystem(SDL_INIT_AUDIO);
    }

    void resetDevice()
    {
        SDL_PauseAudioDevice(device, 1);
        SDL_ClearQueuedAudio(device);
    }

    void run(std::stop_token stop)
    {
        constexpr auto queueBytes = 2 * blockFrames * frameBytes;
        std::unique_lock lock(mutex);
        while (!stop.stop_requested()) {
            if (!active) {
                wake.wait_for(lock, std::chrono::milliseconds(10));
                continue;
            }
            if (written >= end || SDL_GetQueuedAudioSize(device) >= queueBytes) {
                if (preroll) {
                    SDL_PauseAudioDevice(device, 0);
                    preroll = false;
                }
                wake.wait_for(lock, std::chrono::milliseconds(5));
                continue;
            }

            const auto generation = epoch;
            const auto first = written;
            const int count = static_cast<int>(std::min<Time>(blockFrames, end - first));
            lock.unlock();

            std::array<std::int16_t, blockFrames * channels> output{};
            std::string failure;
            try {
                std::array<float, blockFrames * channels> mixed{};
                for (auto& track : tracks)
                    track.mix(first, count, mixed);
                for (int i = 0; i < count * channels; ++i)
                    output[i] = static_cast<std::int16_t>(std::clamp(mixed[i], -32768.0f, 32767.0f));
            } catch (const std::exception& exception) {
                failure = exception.what();
            }

            lock.lock();
            if (generation != epoch || stop.stop_requested())
                continue;
            if (failure.empty() && SDL_QueueAudio(device, output.data(), count * frameBytes) != 0)
                failure = SDL_GetError();
            if (!failure.empty()) {
                error = std::move(failure);
                active = false;
                resetDevice();
            } else {
                written += count;
            }
        }
    }
};

AudioMixer::AudioMixer(const std::vector<Track>& tracks, Time duration)
    : state_(std::make_unique<State>(tracks, duration)) {}
AudioMixer::~AudioMixer() = default;

void AudioMixer::start(Time position)
{
    std::lock_guard lock(state_->mutex);
    if (!state_->device) {
        state_->silentPosition = position;
        state_->silentStart = std::chrono::steady_clock::now();
        state_->active = true;
        return;
    }
    state_->resetDevice();
    ++state_->epoch;
    state_->written = std::clamp(toFrames(position), Time{0}, state_->end);
    state_->error.clear();
    state_->active = true;
    state_->preroll = true;
    state_->wake.notify_one();
}

void AudioMixer::stop()
{
    std::lock_guard lock(state_->mutex);
    ++state_->epoch;
    state_->active = false;
    if (!state_->device)
        return;
    state_->resetDevice();
    state_->wake.notify_one();
}

Time AudioMixer::position() const
{
    std::lock_guard lock(state_->mutex);
    if (!state_->device) {
        const auto elapsed = state_->active
            ? std::chrono::duration_cast<std::chrono::microseconds>(
                std::chrono::steady_clock::now() - state_->silentStart).count()
            : Time{0};
        return std::min(state_->duration, state_->silentPosition + elapsed);
    }
    if (!state_->error.empty())
        throw std::runtime_error(state_->error);
    const auto queued = static_cast<Time>(SDL_GetQueuedAudioSize(state_->device) / frameBytes);
    const auto consumed = state_->written - queued;
    return consumed >= state_->end ? state_->duration : toTime(consumed);
}
}
