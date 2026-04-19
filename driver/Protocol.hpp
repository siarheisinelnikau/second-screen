#pragma once
#include <cstddef>
#include <cstdint>

// Wire ABI v1. Little endian, fixed-width fields, no pointers or OS handles.
namespace secondscreen {
inline constexpr std::uint32_t ProtocolVersion = 0x00010000;
inline constexpr std::uint32_t MaxWidth = 1920, MaxHeight = 1080, MaxFps = 20;
inline constexpr std::size_t MaxPixels = std::size_t(MaxWidth) * MaxHeight * 4;
inline constexpr std::uint32_t Bgra8 = 1;
// CTL_CODE(FILE_DEVICE_UNKNOWN, function, METHOD_BUFFERED, FILE_READ_DATA|FILE_WRITE_DATA).
inline constexpr std::uint32_t QueryIoctl = 0x0022e000;
inline constexpr std::uint32_t CreateIoctl = 0x0022e004;
inline constexpr std::uint32_t FrameIoctl = 0x0022e008;
inline constexpr std::uint32_t RemoveIoctl = 0x0022e00c;
enum class FrameState : std::uint32_t { Pending = 0, Ready = 1, Inactive = 2, Failed = 3 };
struct CreateRequest {
    std::uint32_t version, size, width, height, fps, reserved[3];
};
struct FrameRequest {
    std::uint32_t version, size;
    std::uint64_t generation, afterSequence, reserved;
};
struct FrameHeader {
    std::uint32_t version, size;
    std::uint64_t generation, sequence;
    std::uint32_t width, height, stride, payloadBytes, format;
    FrameState state;
    std::uint32_t error, reserved[3];
};
static_assert(sizeof(CreateRequest) == 32 && sizeof(FrameRequest) == 32);
static_assert(sizeof(FrameHeader) == 64 && offsetof(FrameHeader, error) == 48);
inline bool Valid(const CreateRequest& request) {
    return request.version == ProtocolVersion && request.size == sizeof(request)
        && request.width >= 320 && request.width <= MaxWidth
        && request.height >= 240 && request.height <= MaxHeight
        && request.fps >= 5 && request.fps <= MaxFps
        && !(request.reserved[0] | request.reserved[1] | request.reserved[2]);
}
inline bool Valid(const FrameRequest& request) {
    return request.version == ProtocolVersion && request.size == sizeof(request) && request.reserved == 0;
}
inline FrameHeader Header(std::uint64_t generation, std::uint32_t width, std::uint32_t height) {
    FrameHeader result{};
    result.version = ProtocolVersion; result.size = sizeof(result);
    result.generation = generation; result.width = width; result.height = height;
    result.stride = width * 4; result.format = Bgra8;
    return result;
}
} // namespace secondscreen
