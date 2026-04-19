#include "GpuReadback.hpp"

using Microsoft::WRL::ComPtr;
namespace secondscreen {
HRESULT GpuReadback::Initialize(LUID renderAdapter) {
    ComPtr<IDXGIFactory4> factory;
    HRESULT hr = CreateDXGIFactory2(0, IID_PPV_ARGS(&factory));
    if (FAILED(hr)) return hr;
    ComPtr<IDXGIAdapter> adapter;
    hr = factory->EnumAdapterByLuid(renderAdapter, IID_PPV_ARGS(&adapter));
    if (FAILED(hr)) return hr;
    hr = D3D11CreateDevice(adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        nullptr, 0, D3D11_SDK_VERSION, &device_, nullptr, &context_);
    if (FAILED(hr)) return hr;
    D3D11_TEXTURE2D_DESC desc{};
    desc.Width = width_; desc.Height = height_; desc.MipLevels = 1; desc.ArraySize = 1;
    desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM; desc.SampleDesc.Count = 1;
    desc.Usage = D3D11_USAGE_STAGING; desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    D3D11_QUERY_DESC query{}; query.Query = D3D11_QUERY_EVENT;
    for (auto& slot : slots_) {
        hr = device_->CreateTexture2D(&desc, nullptr, &slot.texture);
        if (FAILED(hr)) return hr;
        hr = device_->CreateQuery(&query, &slot.completion);
        if (FAILED(hr)) return hr;
    }
    return S_OK;
}
HRESULT GpuReadback::Submit(IDXGIResource* resource) {
    if (!resource) return E_INVALIDARG;
    const auto now = std::chrono::steady_clock::now();
    if (now < next_) return S_FALSE;
    Slot* available = nullptr;
    for (auto& slot : slots_) if (!slot.pending) { available = &slot; break; }
    if (!available) return S_FALSE;
    ComPtr<ID3D11Texture2D> texture;
    HRESULT hr = resource->QueryInterface(IID_PPV_ARGS(&texture));
    if (FAILED(hr)) return hr;
    D3D11_TEXTURE2D_DESC desc{}; texture->GetDesc(&desc);
    if (desc.Width != width_ || desc.Height != height_ || desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM || desc.SampleDesc.Count != 1 || desc.MipLevels != 1 || desc.ArraySize != 1) return E_INVALIDARG;
    context_->CopyResource(available->texture.Get(), texture.Get());
    context_->End(available->completion.Get()); context_->Flush();
    available->pending = true; available->order = ++order_;
    next_ = now + std::chrono::microseconds(1000000 / fps_);
    return S_OK;
}
HRESULT GpuReadback::Drain(FrameMailbox& mailbox) {
    for (auto& slot : slots_) {
        if (!slot.pending) continue;
        HRESULT hr = context_->GetData(slot.completion.Get(), nullptr, 0, D3D11_ASYNC_GETDATA_DONOTFLUSH);
        if (hr == S_FALSE) continue;
        if (FAILED(hr)) return hr;
        D3D11_MAPPED_SUBRESOURCE mapped{};
        hr = context_->Map(slot.texture.Get(), 0, D3D11_MAP_READ, D3D11_MAP_FLAG_DO_NOT_WAIT, &mapped);
        if (hr == DXGI_ERROR_WAS_STILL_DRAWING) continue;
        if (FAILED(hr)) return hr;
        bool valid = true;
        try {
            if (slot.order > published_) {
                valid = mailbox.Publish(mapped.pData, mapped.RowPitch, width_, height_);
                if (valid) published_ = slot.order;
            }
        } catch (...) { context_->Unmap(slot.texture.Get(), 0); throw; }
        context_->Unmap(slot.texture.Get(), 0); slot.pending = false;
        if (!valid) return E_ABORT;
    }
    return S_OK;
}
} // namespace secondscreen
