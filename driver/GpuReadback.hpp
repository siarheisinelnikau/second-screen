#pragma once
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <d3d11.h>
#include <dxgi1_4.h>
#include <wrl/client.h>
#include <array>
#include <chrono>
#include "FrameMailbox.hpp"

namespace secondscreen {
// Two staging textures; never wait on a network consumer or on GPU completion.
class GpuReadback {
    struct Slot {
        Microsoft::WRL::ComPtr<ID3D11Texture2D> texture;
        Microsoft::WRL::ComPtr<ID3D11Query> completion;
        std::uint64_t order = 0;
        bool pending = false;
    };
    Microsoft::WRL::ComPtr<ID3D11Device> device_;
    Microsoft::WRL::ComPtr<ID3D11DeviceContext> context_;
    std::array<Slot, 2> slots_;
    std::uint32_t width_, height_, fps_;
    std::uint64_t order_ = 0, published_ = 0;
    std::chrono::steady_clock::time_point next_{};
public:
    GpuReadback(std::uint32_t width, std::uint32_t height, std::uint32_t fps) : width_(width), height_(height), fps_(fps) {}
    HRESULT Initialize(LUID renderAdapter);
    HRESULT Device(Microsoft::WRL::ComPtr<IDXGIDevice>& result) const { return device_.As(&result); }
    HRESULT Submit(IDXGIResource* resource);
    HRESULT Drain(FrameMailbox& mailbox);
    bool Pending() const { return slots_[0].pending || slots_[1].pending; }
};
} // namespace secondscreen
