#pragma once
#include "Protocol.hpp"
#include <cstring>
#include <memory>
#include <mutex>
#include <vector>

namespace secondscreen {
enum class CopyResult { Ok, InvalidRequest, TooSmall };
// Independent of WDF/D3D: the SDK-only tests exercise the actual driver mailbox.
class FrameMailbox {
    struct Pixels { std::uint64_t sequence; std::vector<std::uint8_t> bytes; };
    const std::uint64_t generation_;
    const std::uint32_t width_, height_;
    std::mutex mutex_;
    std::shared_ptr<const Pixels> latest_;
    FrameState state_ = FrameState::Pending;
    std::uint32_t error_ = 0;
    std::uint64_t sequence_ = 0;
    bool closed_ = false;
public:
    FrameMailbox(std::uint64_t generation, std::uint32_t width, std::uint32_t height)
        : generation_(generation), width_(width), height_(height) {}
    bool Publish(const void* bgra, std::uint32_t pitch, std::uint32_t width, std::uint32_t height) {
        if (!bgra || width != width_ || height != height_ || width > MaxWidth || height > MaxHeight || pitch < width * 4) return false;
        auto pixels = std::make_shared<Pixels>();
        pixels->bytes.resize(std::size_t(width) * height * 4);
        for (std::uint32_t row = 0; row < height; ++row)
            std::memcpy(pixels->bytes.data() + std::size_t(row) * width * 4,
                static_cast<const std::uint8_t*>(bgra) + std::size_t(row) * pitch, std::size_t(width) * 4);
        std::lock_guard<std::mutex> guard(mutex_);
        if (closed_ || state_ == FrameState::Inactive || state_ == FrameState::Failed) return false;
        pixels->sequence = ++sequence_; latest_ = std::move(pixels); state_ = FrameState::Ready;
        return true;
    }
    void Close() { std::lock_guard<std::mutex> guard(mutex_); closed_ = true; state_ = FrameState::Inactive; error_ = 0; latest_.reset(); }
    void Suspend() { std::lock_guard<std::mutex> guard(mutex_); state_ = FrameState::Inactive; error_ = 0; latest_.reset(); }
    void Fail(std::uint32_t error) { std::lock_guard<std::mutex> guard(mutex_); if (!closed_ && state_ != FrameState::Inactive) { state_ = FrameState::Failed; error_ = error; latest_.reset(); } }
    void Reset() { std::lock_guard<std::mutex> guard(mutex_); if (!closed_) { state_ = FrameState::Pending; error_ = 0; latest_.reset(); } }
    CopyResult Copy(const FrameRequest& request, void* output, std::size_t capacity, std::size_t& written) {
        written = 0;
        if (!Valid(request) || request.generation != generation_) return CopyResult::InvalidRequest;
        if (!output || capacity < sizeof(FrameHeader)) return CopyResult::TooSmall;
        auto header = Header(generation_, width_, height_);
        std::shared_ptr<const Pixels> pixels;
        {
            std::lock_guard<std::mutex> guard(mutex_);
            header.state = state_; header.error = error_; header.sequence = sequence_;
            if (latest_ && latest_->sequence > request.afterSequence) pixels = latest_;
            else if (header.state == FrameState::Ready) header.state = FrameState::Pending;
        }
        if (pixels) {
            header.payloadBytes = static_cast<std::uint32_t>(pixels->bytes.size());
            if (capacity < sizeof(header) + pixels->bytes.size()) return CopyResult::TooSmall;
            std::memcpy(static_cast<std::uint8_t*>(output) + sizeof(header), pixels->bytes.data(), pixels->bytes.size());
        }
        std::memcpy(output, &header, sizeof(header)); written = sizeof(header) + header.payloadBytes;
        return CopyResult::Ok;
    }
};
} // namespace secondscreen
