#pragma once

#include <cstdint>
#include <filesystem>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace meditor {
using Time = std::int64_t; // Microseconds in the relevant source or timeline.
inline constexpr Time second = 1'000'000;
inline constexpr Time maximumDuration = 24 * 60 * 60 * second;

struct Caption {
    std::string id;
    Time start;
    Time end;
    std::string speaker;
    std::string text;
};

struct Track {
    std::filesystem::path path;
    std::string name;
    Time offset;
    double gain;
};

struct ClipRange {
    Time start;
    Time end;

    bool validFor(Time sourceDuration) const;
    Time duration() const { return end - start; }
    bool operator==(const ClipRange&) const = default;
};

struct Project {
    std::filesystem::path video;
    std::filesystem::path proxy;
    std::vector<Track> audio;
    std::vector<Caption> captions;
    std::optional<ClipRange> clip;

    bool setClip(ClipRange range, Time sourceDuration);
    ClipRange playbackRange(Time sourceDuration) const { return clip.value_or(ClipRange{0, sourceDuration}); }

    static Project load(const std::filesystem::path& file);
    void save(const std::filesystem::path& file) const;
};

std::string utf8(const std::filesystem::path& path);
std::filesystem::path fromUtf8(std::string_view value);
std::vector<Caption> importSrt(const std::filesystem::path& file);
}
