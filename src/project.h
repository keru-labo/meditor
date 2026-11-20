#pragma once

#include <cstdint>
#include <filesystem>
#include <string>
#include <string_view>
#include <vector>

namespace meditor {
using Time = std::int64_t; // Project-relative microseconds.
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

struct Project {
    std::filesystem::path video;
    std::filesystem::path proxy;
    std::vector<Track> audio;
    std::vector<Caption> captions;

    static Project load(const std::filesystem::path& file);
    void save(const std::filesystem::path& file) const;
};

std::string utf8(const std::filesystem::path& path);
std::filesystem::path fromUtf8(std::string_view value);
std::vector<Caption> importSrt(const std::filesystem::path& file);
}
