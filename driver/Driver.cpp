// SecondScreen's IddCx/UMDF adapter. No monitor arrives until an owning file requests it.
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <wudfwdm.h>
#include <wdf.h>
#include <iddcx.h>
#include <memory>
#include <mutex>
#include <new>
#include <thread>
#include "GpuReadback.hpp"

using namespace secondscreen;
using Microsoft::WRL::ComPtr;
static const GUID InterfaceGuid = {0x89e573d9, 0x1361, 0x48d7, {0xbd, 0x37, 0x3c, 0x8e, 0x55, 0x98, 0xef, 0x4a}};
static const GUID MonitorContainer = {0xb7c826dd, 0x09be, 0x4b64, {0xa4, 0xa9, 0x41, 0x55, 0xac, 0x13, 0xbe, 0x2b}};

extern "C" DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD DeviceAdd;
EVT_WDF_DEVICE_D0_ENTRY D0Entry;
EVT_WDF_DEVICE_D0_EXIT D0Exit;
EVT_WDF_DEVICE_FILE_CREATE FileCreate;
EVT_WDF_FILE_CLEANUP FileCleanup;
EVT_WDF_TIMER Watchdog;
EVT_IDD_CX_DEVICE_IO_CONTROL IoControl;
EVT_IDD_CX_ADAPTER_INIT_FINISHED AdapterReady;
EVT_IDD_CX_ADAPTER_COMMIT_MODES CommitModes;
EVT_IDD_CX_PARSE_MONITOR_DESCRIPTION ParseDescription;
EVT_IDD_CX_MONITOR_GET_DEFAULT_DESCRIPTION_MODES DefaultModes;
EVT_IDD_CX_MONITOR_QUERY_TARGET_MODES TargetModes;
EVT_IDD_CX_MONITOR_ASSIGN_SWAPCHAIN AssignSwapchain;
EVT_IDD_CX_MONITOR_UNASSIGN_SWAPCHAIN UnassignSwapchain;

class SwapchainWorker {
    IDDCX_SWAPCHAIN chain_;
    HANDLE available_, stop_ = nullptr;
    std::shared_ptr<FrameMailbox> mailbox_;
    GpuReadback gpu_;
    std::thread thread_;
    void Run() noexcept {
        HRESULT hr = S_OK;
        try {
            ComPtr<IDXGIDevice> device;
            hr = gpu_.Device(device);
            if (SUCCEEDED(hr)) {
                IDARG_IN_SWAPCHAINSETDEVICE args{}; args.pDevice = device.Get();
                hr = IddCxSwapChainSetDevice(chain_, &args);
            }
            while (SUCCEEDED(hr) && WaitForSingleObject(stop_, 0) == WAIT_TIMEOUT) {
                hr = gpu_.Drain(*mailbox_);
                if (FAILED(hr)) break;
                IDARG_OUT_RELEASEANDACQUIREBUFFER acquired{};
                hr = IddCxSwapChainReleaseAndAcquireBuffer(chain_, &acquired);
                if (hr == E_PENDING) {
                    HANDLE waits[] = {stop_, available_};
                    const DWORD result = WaitForMultipleObjects(2, waits, FALSE, gpu_.Pending() ? 16 : INFINITE);
                    if (result == WAIT_OBJECT_0) { hr = S_OK; break; }
                    if (result != WAIT_OBJECT_0 + 1 && result != WAIT_TIMEOUT) { hr = HRESULT_FROM_WIN32(GetLastError()); break; }
                    hr = S_OK; continue;
                }
                if (FAILED(hr)) break;
                // Attach releases exactly the reference supplied by IddCx. The GPU copy
                // retains its resource internally until commands finish; no surface is queued.
                ComPtr<IDXGIResource> resource; resource.Attach(acquired.MetaData.pSurface);
                hr = gpu_.Submit(resource.Get());
                resource.Reset();
                const HRESULT finished = IddCxSwapChainFinishedProcessingFrame(chain_);
                if (SUCCEEDED(hr)) hr = finished;
            }
        } catch (const std::bad_alloc&) { hr = E_OUTOFMEMORY; }
        catch (...) { hr = E_FAIL; }
        if (FAILED(hr) && WaitForSingleObject(stop_, 0) == WAIT_TIMEOUT) mailbox_->Fail(static_cast<std::uint32_t>(hr));
        // Ownership transfers on a successful AssignSwapchain; only this worker deletes it.
        WdfObjectDelete(chain_);
    }
public:
    SwapchainWorker(IDDCX_SWAPCHAIN chain, HANDLE available, const CreateRequest& mode, std::shared_ptr<FrameMailbox> mailbox)
        : chain_(chain), available_(available), mailbox_(std::move(mailbox)), gpu_(mode.width, mode.height, mode.fps) {}
    HRESULT Start(LUID adapter) {
        stop_ = CreateEventW(nullptr, TRUE, FALSE, nullptr);
        if (!stop_) return HRESULT_FROM_WIN32(GetLastError());
        HRESULT hr = gpu_.Initialize(adapter);
        if (FAILED(hr)) return hr;
        try { thread_ = std::thread([this] { Run(); }); }
        catch (...) { return E_OUTOFMEMORY; }
        return S_OK;
    }
    ~SwapchainWorker() {
        if (stop_) SetEvent(stop_);
        if (thread_.joinable()) thread_.join();
        if (stop_) CloseHandle(stop_);
    }
};

struct MonitorState {
    CreateRequest mode;
    std::shared_ptr<FrameMailbox> mailbox;
    WDFTIMER watchdog = nullptr;
    ULONGLONG lastRead = 0;
    std::mutex mutex;
    std::unique_ptr<SwapchainWorker> worker;
    MonitorState(CreateRequest request, std::shared_ptr<FrameMailbox> frames) : mode(request), mailbox(std::move(frames)) {}
    void Stop() { std::unique_ptr<SwapchainWorker> previous; { std::lock_guard<std::mutex> lock(mutex); previous = std::move(worker); } }
    ~MonitorState() { Stop(); }
};
struct DeviceState {
    std::mutex control;
    IDDCX_ADAPTER adapter = nullptr;
    IDDCX_MONITOR monitor = nullptr;
    WDFFILEOBJECT owner = nullptr;
    bool ready = false, powered = false;
    std::uint64_t generation = 0;
    std::shared_ptr<FrameMailbox> mailbox;
    // Caller holds control. Monitor callbacks never acquire this device mutex.
    NTSTATUS Remove() {
        if (mailbox) mailbox->Close();
        const NTSTATUS status = monitor ? IddCxMonitorDeparture(monitor) : STATUS_SUCCESS;
        owner = nullptr;
        if (NT_SUCCESS(status)) {
            monitor = nullptr; mailbox.reset();
            if (watchdog) WdfTimerStop(watchdog, FALSE);
        } else {
            // Keep the handle until departure succeeds; never lose a connected monitor.
            lastRead = GetTickCount64() - 5000;
            if (watchdog) WdfTimerStart(watchdog, WDF_REL_TIMEOUT_IN_MS(1000));
        }
        return status;
    }
};
struct DeviceContext { std::shared_ptr<DeviceState>* value; };
struct MonitorContext { MonitorState* value; };
WDF_DECLARE_CONTEXT_TYPE(DeviceContext);
WDF_DECLARE_CONTEXT_TYPE(MonitorContext);
static DeviceState& Device(WDFOBJECT object) { return **WdfObjectGet_DeviceContext(object)->value; }
static MonitorState& Monitor(IDDCX_MONITOR object) { return *WdfObjectGet_MonitorContext(object)->value; }

extern "C" BOOL WINAPI DllMain(HINSTANCE, DWORD, LPVOID) { return TRUE; }
_Use_decl_annotations_
extern "C" NTSTATUS DriverEntry(PDRIVER_OBJECT object, PUNICODE_STRING registry) {
    WDF_DRIVER_CONFIG config; WDF_DRIVER_CONFIG_INIT(&config, DeviceAdd);
    return WdfDriverCreate(object, registry, WDF_NO_OBJECT_ATTRIBUTES, &config, WDF_NO_HANDLE);
}
_Use_decl_annotations_
NTSTATUS DeviceAdd(WDFDRIVER, PWDFDEVICE_INIT init) {
    WDF_PNPPOWER_EVENT_CALLBACKS power; WDF_PNPPOWER_EVENT_CALLBACKS_INIT(&power);
    power.EvtDeviceD0Entry = D0Entry; power.EvtDeviceD0Exit = D0Exit;
    WdfDeviceInitSetPnpPowerEventCallbacks(init, &power);
    WDF_FILEOBJECT_CONFIG files; WDF_FILEOBJECT_CONFIG_INIT(&files, FileCreate, WDF_NO_EVENT_CALLBACK, FileCleanup);
    files.FileObjectClass = static_cast<WDF_FILEOBJECT_CLASS>(WdfFileObjectWdfCannotUseFsContexts | WdfFileObjectCanBeOptional);
    WdfDeviceInitSetFileObjectConfig(init, &files, WDF_NO_OBJECT_ATTRIBUTES);
    IDD_CX_CLIENT_CONFIG idd; IDD_CX_CLIENT_CONFIG_INIT(&idd);
    idd.EvtIddCxDeviceIoControl = IoControl;
    idd.EvtIddCxAdapterInitFinished = AdapterReady; idd.EvtIddCxAdapterCommitModes = CommitModes;
    idd.EvtIddCxParseMonitorDescription = ParseDescription;
    idd.EvtIddCxMonitorGetDefaultDescriptionModes = DefaultModes;
    idd.EvtIddCxMonitorQueryTargetModes = TargetModes;
    idd.EvtIddCxMonitorAssignSwapChain = AssignSwapchain; idd.EvtIddCxMonitorUnassignSwapChain = UnassignSwapchain;
    NTSTATUS status = IddCxDeviceInitConfig(init, &idd);
    if (!NT_SUCCESS(status)) return status;
    WDF_OBJECT_ATTRIBUTES attributes; WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, DeviceContext);
    attributes.ExecutionLevel = WdfExecutionLevelPassive;
    attributes.SynchronizationScope = WdfSynchronizationScopeNone;
    attributes.EvtCleanupCallback = [](WDFOBJECT object) { delete WdfObjectGet_DeviceContext(object)->value; };
    WDFDEVICE device;
    status = WdfDeviceCreate(&init, &attributes, &device);
    if (!NT_SUCCESS(status)) return status;
    try { WdfObjectGet_DeviceContext(device)->value = new std::shared_ptr<DeviceState>(std::make_shared<DeviceState>()); }
    catch (...) { return STATUS_INSUFFICIENT_RESOURCES; }
    WDF_TIMER_CONFIG timer; WDF_TIMER_CONFIG_INIT(&timer, Watchdog); timer.AutomaticSerialization = FALSE;
    WDF_OBJECT_ATTRIBUTES timerAttributes; WDF_OBJECT_ATTRIBUTES_INIT(&timerAttributes);
    timerAttributes.ParentObject = device; timerAttributes.ExecutionLevel = WdfExecutionLevelPassive;
    status = WdfTimerCreate(&timer, &timerAttributes, &Device(device).watchdog);
    if (!NT_SUCCESS(status)) return status;
    status = WdfDeviceCreateDeviceInterface(device, &InterfaceGuid, nullptr);
    if (!NT_SUCCESS(status)) return status;
    return IddCxDeviceInitialize(device);
}
_Use_decl_annotations_
NTSTATUS D0Entry(WDFDEVICE device, WDF_POWER_DEVICE_STATE) {
    auto& state = Device(device);
    std::lock_guard<std::mutex> lock(state.control);
    state.powered = true;
    if (state.adapter) return STATUS_SUCCESS;
    IDDCX_ADAPTER_CAPS caps{}; caps.Size = sizeof(caps); caps.MaxMonitorsSupported = 1;
    caps.EndPointDiagnostics.Size = sizeof(caps.EndPointDiagnostics);
    caps.EndPointDiagnostics.GammaSupport = IDDCX_FEATURE_IMPLEMENTATION_NONE;
    caps.EndPointDiagnostics.TransmissionType = IDDCX_TRANSMISSION_TYPE_WIRED_OTHER;
    caps.EndPointDiagnostics.pEndPointFriendlyName = L"SecondScreen Virtual Display";
    caps.EndPointDiagnostics.pEndPointManufacturerName = L"SecondScreen";
    caps.EndPointDiagnostics.pEndPointModelName = L"SecondScreen IOCTL Display";
    IDDCX_ENDPOINT_VERSION version{}; version.Size = sizeof(version); version.MajorVer = 1;
    caps.EndPointDiagnostics.pFirmwareVersion = &version; caps.EndPointDiagnostics.pHardwareVersion = &version;
    WDF_OBJECT_ATTRIBUTES attributes; WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, DeviceContext);
    attributes.EvtCleanupCallback = [](WDFOBJECT object) { delete WdfObjectGet_DeviceContext(object)->value; };
    IDARG_IN_ADAPTER_INIT input{}; input.WdfDevice = device; input.pCaps = &caps; input.ObjectAttributes = &attributes;
    IDARG_OUT_ADAPTER_INIT output{};
    auto holder = std::unique_ptr<std::shared_ptr<DeviceState>>(new (std::nothrow) std::shared_ptr<DeviceState>(*WdfObjectGet_DeviceContext(device)->value));
    if (!holder) return STATUS_INSUFFICIENT_RESOURCES;
    const NTSTATUS status = IddCxAdapterInitAsync(&input, &output);
    if (NT_SUCCESS(status)) { WdfObjectGet_DeviceContext(output.AdapterObject)->value = holder.release(); state.adapter = output.AdapterObject; }
    return status;
}
_Use_decl_annotations_
NTSTATUS D0Exit(WDFDEVICE device, WDF_POWER_DEVICE_STATE) {
    auto& state = Device(device); std::lock_guard<std::mutex> lock(state.control);
    state.powered = false; return state.Remove();
}
_Use_decl_annotations_
NTSTATUS AdapterReady(IDDCX_ADAPTER adapter, const IDARG_IN_ADAPTER_INIT_FINISHED* input) {
    auto& state = Device(adapter); std::lock_guard<std::mutex> lock(state.control);
    state.ready = NT_SUCCESS(input->AdapterInitStatus); return STATUS_SUCCESS;
}
_Use_decl_annotations_
void FileCreate(WDFDEVICE, WDFREQUEST request, WDFFILEOBJECT file) {
    const auto name = WdfFileObjectGetFileName(file);
    WdfRequestComplete(request, name && name->Length ? STATUS_OBJECT_NAME_INVALID : STATUS_SUCCESS);
}
_Use_decl_annotations_
void FileCleanup(WDFFILEOBJECT file) {
    auto& state = Device(WdfFileObjectGetDevice(file)); std::lock_guard<std::mutex> lock(state.control);
    if (state.owner == file) { const NTSTATUS status = state.Remove(); UNREFERENCED_PARAMETER(status); }
}
_Use_decl_annotations_
void Watchdog(WDFTIMER timer) {
    auto& state = Device(WdfTimerGetParentObject(timer)); std::lock_guard<std::mutex> lock(state.control);
    if (!state.monitor) return;
    if (GetTickCount64() - state.lastRead >= 5000) { const NTSTATUS status = state.Remove(); UNREFERENCED_PARAMETER(status); }
    else WdfTimerStart(timer, WDF_REL_TIMEOUT_IN_MS(1000));
}

static NTSTATUS CreateMonitor(DeviceState& state, WDFFILEOBJECT owner, const CreateRequest& mode) {
    if (!state.ready || !state.powered) return STATUS_DEVICE_NOT_READY;
    if (!owner) return STATUS_ACCESS_DENIED;
    if (state.monitor) return STATUS_DEVICE_BUSY;
    const auto generation = ++state.generation;
    auto mailbox = std::make_shared<FrameMailbox>(generation, mode.width, mode.height);
    auto context = std::make_unique<MonitorState>(mode, mailbox);
    WDF_OBJECT_ATTRIBUTES attributes; WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, MonitorContext);
    attributes.EvtCleanupCallback = [](WDFOBJECT object) { delete WdfObjectGet_MonitorContext(object)->value; };
    IDDCX_MONITOR_INFO info{}; info.Size = sizeof(info);
    info.MonitorType = DISPLAYCONFIG_OUTPUT_TECHNOLOGY_OTHER; info.ConnectorIndex = 0;
    info.MonitorContainerId = MonitorContainer;
    info.MonitorDescription.Size = sizeof(info.MonitorDescription); info.MonitorDescription.Type = IDDCX_MONITOR_DESCRIPTION_TYPE_EDID;
    // EDID-less: modes are provided by DefaultModes, never by copying a real monitor EDID.
    IDARG_IN_MONITORCREATE input{}; input.ObjectAttributes = &attributes; input.pMonitorInfo = &info;
    IDARG_OUT_MONITORCREATE output{};
    NTSTATUS status = IddCxMonitorCreate(state.adapter, &input, &output);
    if (!NT_SUCCESS(status)) return status;
    WdfObjectGet_MonitorContext(output.MonitorObject)->value = context.release();
    IDARG_OUT_MONITORARRIVAL arrival{};
    status = IddCxMonitorArrival(output.MonitorObject, &arrival);
    if (!NT_SUCCESS(status)) { mailbox->Close(); WdfObjectDelete(output.MonitorObject); return status; }
    state.monitor = output.MonitorObject; state.mailbox = std::move(mailbox); state.owner = owner;
    state.lastRead = GetTickCount64(); WdfTimerStart(state.watchdog, WDF_REL_TIMEOUT_IN_MS(1000));
    return STATUS_SUCCESS;
}
_Use_decl_annotations_
void IoControl(WDFDEVICE device, WDFREQUEST request, size_t outputLength, size_t inputLength, ULONG code) {
    NTSTATUS status = STATUS_INVALID_DEVICE_REQUEST;
    std::size_t written = 0;
    try {
        auto& state = Device(device); std::lock_guard<std::mutex> lock(state.control);
        if (code != QueryIoctl && code != CreateIoctl && code != FrameIoctl && code != RemoveIoctl) { WdfRequestComplete(request, status); return; }
        const auto file = WdfRequestGetFileObject(request);
        if (inputLength != sizeof(FrameRequest) || outputLength < sizeof(FrameHeader) || outputLength > sizeof(FrameHeader) + MaxPixels) { WdfRequestComplete(request, STATUS_INVALID_BUFFER_SIZE); return; }
        void* input = nullptr; void* output = nullptr;
        status = WdfRequestRetrieveInputBuffer(request, sizeof(FrameRequest), &input, nullptr);
        if (!NT_SUCCESS(status)) { WdfRequestComplete(request, status); return; }
        // METHOD_BUFFERED can alias input and output. Copy the request before writing.
        FrameRequest frameRequest{}; CreateRequest createRequest{};
        std::memcpy(&frameRequest, input, sizeof(frameRequest)); std::memcpy(&createRequest, input, sizeof(createRequest));
        status = WdfRequestRetrieveOutputBuffer(request, sizeof(FrameHeader), &output, nullptr);
        if (!NT_SUCCESS(status)) { WdfRequestComplete(request, status); return; }
        if (code == CreateIoctl) {
            if (!Valid(createRequest)) status = STATUS_INVALID_PARAMETER;
            else {
                status = CreateMonitor(state, file, createRequest);
                if (NT_SUCCESS(status)) { const auto header = Header(state.generation, createRequest.width, createRequest.height); std::memcpy(output, &header, sizeof(header)); written = sizeof(header); }
            }
        } else if (!Valid(frameRequest)) status = STATUS_INVALID_PARAMETER;
        else if (code == QueryIoctl) {
            if (frameRequest.generation || frameRequest.afterSequence) status = STATUS_INVALID_PARAMETER;
            else { auto header = Header(0, MaxWidth, MaxHeight); header.state = state.ready && state.powered ? FrameState::Pending : FrameState::Inactive; std::memcpy(output, &header, sizeof(header)); written = sizeof(header); status = STATUS_SUCCESS; }
        } else if (!file || state.owner != file) status = STATUS_ACCESS_DENIED;
        else if (!state.mailbox || frameRequest.generation != state.generation) status = STATUS_INVALID_PARAMETER;
        else if (code == RemoveIoctl) {
            status = state.Remove();
            if (NT_SUCCESS(status)) { auto header = Header(frameRequest.generation, 0, 0); header.state = FrameState::Inactive; std::memcpy(output, &header, sizeof(header)); written = sizeof(header); }
        } else {
            state.lastRead = GetTickCount64();
            const auto result = state.mailbox->Copy(frameRequest, output, outputLength, written);
            status = result == CopyResult::Ok ? STATUS_SUCCESS : result == CopyResult::TooSmall ? STATUS_BUFFER_TOO_SMALL : STATUS_INVALID_PARAMETER;
        }
    } catch (const std::bad_alloc&) { status = STATUS_INSUFFICIENT_RESOURCES; }
    catch (...) { status = STATUS_UNSUCCESSFUL; }
    WdfRequestCompleteWithInformation(request, status, NT_SUCCESS(status) ? written : 0);
}

static void Signal(DISPLAYCONFIG_VIDEO_SIGNAL_INFO& signal, UINT width, UINT height, bool monitorMode) {
    signal.activeSize.cx = signal.totalSize.cx = width; signal.activeSize.cy = signal.totalSize.cy = height;
    signal.vSyncFreq.Numerator = 60; signal.vSyncFreq.Denominator = 1;
    signal.hSyncFreq.Numerator = 60 * height; signal.hSyncFreq.Denominator = 1;
    signal.pixelRate = UINT64(width) * height * 60;
    signal.AdditionalSignalInfo.vSyncFreqDivider = monitorMode ? 0 : 1;
    signal.AdditionalSignalInfo.videoStandard = 255;
    signal.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
}
_Use_decl_annotations_
NTSTATUS ParseDescription(const IDARG_IN_PARSEMONITORDESCRIPTION*, IDARG_OUT_PARSEMONITORDESCRIPTION*) { return STATUS_NOT_SUPPORTED; }
_Use_decl_annotations_
NTSTATUS DefaultModes(IDDCX_MONITOR object, const IDARG_IN_GETDEFAULTDESCRIPTIONMODES* input, IDARG_OUT_GETDEFAULTDESCRIPTIONMODES* output) {
    output->DefaultMonitorModeBufferOutputCount = 1; output->PreferredMonitorModeIdx = 0;
    if (input->DefaultMonitorModeBufferInputCount == 0) return STATUS_SUCCESS;
    auto& mode = input->pDefaultMonitorModes[0]; mode = {}; mode.Size = sizeof(mode); mode.Origin = IDDCX_MONITOR_MODE_ORIGIN_DRIVER;
    const auto& config = Monitor(object).mode; Signal(mode.MonitorVideoSignalInfo, config.width, config.height, true);
    return STATUS_SUCCESS;
}
_Use_decl_annotations_
NTSTATUS TargetModes(IDDCX_MONITOR object, const IDARG_IN_QUERYTARGETMODES* input, IDARG_OUT_QUERYTARGETMODES* output) {
    output->TargetModeBufferOutputCount = 1;
    if (input->TargetModeBufferInputCount == 0) return STATUS_SUCCESS;
    auto& mode = input->pTargetModes[0]; mode = {}; mode.Size = sizeof(mode);
    const auto& config = Monitor(object).mode; Signal(mode.TargetVideoSignalInfo.targetVideoSignalInfo, config.width, config.height, false);
    return STATUS_SUCCESS;
}
_Use_decl_annotations_
NTSTATUS CommitModes(IDDCX_ADAPTER, const IDARG_IN_COMMITMODES* input) {
    for (UINT i = 0; i < input->PathCount; ++i) {
        const auto& path = input->pPaths[i];
        auto& monitor = Monitor(path.MonitorObject);
        if (!(path.Flags & IDDCX_PATH_FLAGS_ACTIVE)) { monitor.mailbox->Suspend(); continue; }
        const auto& active = path.TargetVideoSignalInfo.activeSize;
        if (active.cx != monitor.mode.width || active.cy != monitor.mode.height) return STATUS_INVALID_PARAMETER;
    }
    return STATUS_SUCCESS;
}
_Use_decl_annotations_
NTSTATUS AssignSwapchain(IDDCX_MONITOR object, const IDARG_IN_SETSWAPCHAIN* input) {
    auto& monitor = Monitor(object);
    std::lock_guard<std::mutex> lock(monitor.mutex);
    monitor.worker.reset(); monitor.mailbox->Reset();
    try {
        auto worker = std::make_unique<SwapchainWorker>(input->hSwapChain, input->hNextSurfaceAvailable, monitor.mode, monitor.mailbox);
        const HRESULT hr = worker->Start(input->RenderAdapterLuid);
        if (FAILED(hr)) { monitor.mailbox->Fail(static_cast<std::uint32_t>(hr)); return STATUS_GRAPHICS_INDIRECT_DISPLAY_ABANDON_SWAPCHAIN; }
        monitor.worker = std::move(worker); return STATUS_SUCCESS;
    } catch (...) { monitor.mailbox->Fail(static_cast<std::uint32_t>(E_OUTOFMEMORY)); return STATUS_GRAPHICS_INDIRECT_DISPLAY_ABANDON_SWAPCHAIN; }
}
_Use_decl_annotations_
NTSTATUS UnassignSwapchain(IDDCX_MONITOR object) { auto& monitor = Monitor(object); monitor.Stop(); monitor.mailbox->Suspend(); return STATUS_SUCCESS; }
