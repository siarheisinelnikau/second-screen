#include <windows.h>
#include <winioctl.h>
#include <cassert>
#include <fstream>
#include <iostream>
#include "../FrameMailbox.hpp"
using namespace secondscreen;

int main(int argc, char** argv) {
    assert(!ReaderExpired(5999, 1000)); assert(ReaderExpired(6000, 1000));
    static_assert(QueryIoctl == CTL_CODE(FILE_DEVICE_UNKNOWN, 0x800, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA));
    static_assert(CreateIoctl == CTL_CODE(FILE_DEVICE_UNKNOWN, 0x801, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA));
    static_assert(FrameIoctl == CTL_CODE(FILE_DEVICE_UNKNOWN, 0x802, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA));
    static_assert(RemoveIoctl == CTL_CODE(FILE_DEVICE_UNKNOWN, 0x803, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA));
    CreateRequest create{ProtocolVersion, 32, 320, 240, 10, {0, 0, 0}};
    assert(Valid(create)); create.width = UINT32_MAX; assert(!Valid(create)); create.width = 320;
    create.reserved[1] = 1; assert(!Valid(create));
    FrameMailbox mailbox(7, 320, 240);
    FrameRequest request{ProtocolVersion, 32, 7, 0, 0};
    std::vector<std::uint8_t> output(sizeof(FrameHeader) + 320 * 240 * 4);
    std::size_t written = 0;
    assert(mailbox.Copy(request, output.data(), output.size(), written) == CopyResult::Ok && written == 64);
    FrameHeader header{}; std::memcpy(&header, output.data(), 64); assert(header.state == FrameState::Pending);
    constexpr std::uint32_t pitch = 320 * 4 + 16;
    std::vector<std::uint8_t> padded(pitch * 240, 0xee);
    for (unsigned y = 0; y < 240; ++y) for (unsigned x = 0; x < 320; ++x) {
        auto* pixel = padded.data() + y * pitch + x * 4; pixel[0] = 10; pixel[1] = 20; pixel[2] = 30; pixel[3] = 255;
    }
    assert(!mailbox.Publish(padded.data(), 10, 320, 240));
    assert(mailbox.Publish(padded.data(), pitch, 320, 240));
    assert(mailbox.Copy(request, output.data(), 64, written) == CopyResult::TooSmall && written == 0);
    assert(mailbox.Copy(request, output.data(), output.size(), written) == CopyResult::Ok);
    std::memcpy(&header, output.data(), 64);
    assert(header.sequence == 1 && header.payloadBytes == 320 * 240 * 4 && header.stride == 1280);
    assert(output[64] == 10 && output[64 + 1280] == 10 && output.back() == 255);
    if (argc == 2) { std::ofstream fixture(argv[1], std::ios::binary); fixture.write(reinterpret_cast<const char*>(output.data()), static_cast<std::streamsize>(written)); assert(fixture.good()); }
    request.afterSequence = 1;
    assert(mailbox.Copy(request, output.data(), output.size(), written) == CopyResult::Ok && written == 64);
    request.generation = 8; assert(mailbox.Copy(request, output.data(), output.size(), written) == CopyResult::InvalidRequest);
    request.generation = 7; request.reserved = 1; assert(mailbox.Copy(request, output.data(), output.size(), written) == CopyResult::InvalidRequest);
    request.reserved = 0; mailbox.Suspend();
    assert(mailbox.Copy(request, output.data(), output.size(), written) == CopyResult::Ok);
    std::memcpy(&header, output.data(), 64); assert(header.state == FrameState::Inactive && header.payloadBytes == 0);
    mailbox.Reset(); assert(mailbox.Publish(padded.data(), pitch, 320, 240));
    mailbox.Close(); mailbox.Reset(); mailbox.Fail(123);
    assert(!mailbox.Publish(padded.data(), pitch, 320, 240));
    assert(mailbox.Copy(request, output.data(), output.size(), written) == CopyResult::Ok);
    std::memcpy(&header, output.data(), 64); assert(header.state == FrameState::Inactive && header.payloadBytes == 0);
    std::cout << "Driver protocol, padded BGRA copy, bounds, generation and lifecycle checks passed.\n";
}
