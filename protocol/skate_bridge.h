#pragma once

#include <cstdint>
#include <cstddef>

namespace skyrim_skate::proto
{
    inline constexpr std::uint32_t kMagic = 0x4B534B53; // "SKSK"
    inline constexpr std::uint32_t kVersion = 1;
    inline constexpr wchar_t kMappingName[] = L"Local\\SkyCraftSkate_v1";

    inline constexpr std::uint64_t kOffHeader = 0x0000;
    inline constexpr std::uint64_t kOffSkyState = 0x0100;
    inline constexpr std::uint64_t kOffHostState = 0x0200;
    inline constexpr std::uint64_t kOffCollisionRing = 0x1000;
    inline constexpr std::uint64_t kCollisionRingBytes = 32ull << 20;
    inline constexpr std::uint64_t kMappingBytes = kOffCollisionRing + kCollisionRingBytes;

    struct Header
    {
        std::uint32_t magic;
        std::uint32_t version;
        std::uint32_t skyrimPid;
        std::uint32_t hostPid;
        std::uint64_t skyrimHeartbeatMs;
        std::uint64_t hostHeartbeatMs;
        std::uint8_t reserved[0x40 - 0x20];
    };
    static_assert(sizeof(Header) == 0x40);

    enum SkyFlags : std::uint32_t
    {
        kSkyInGame = 1u << 0,
        kSkyMenuOpen = 1u << 1,
        kSkyLoading = 1u << 2,
        kSkySkateRequested = 1u << 3,
        kSkyMinecraftPresent = 1u << 4,
    };

    struct SkyState
    {
        std::uint32_t seq;              // seqlock: odd while writing
        std::uint32_t flags;
        std::uint32_t collisionEpoch;   // changes on cell/world reset
        std::uint32_t toggleSeq;        // increments for each requested authority toggle

        double x, y, z;                 // SkyCraft/Minecraft space; metres for Skate bridge
        float yawDeg;
        float pitchDeg;
        float aspectRatio;
        std::uint32_t viewportW;
        std::uint32_t viewportH;

        std::uint8_t reserved[0x80 - 0x3C];
    };
    static_assert(sizeof(SkyState) == 0x80);

    enum HostFlags : std::uint32_t
    {
        kHostReady = 1u << 0,
        kHostSkateActive = 1u << 1,
        kHostHasPose = 1u << 2,
        kHostHasCamera = 1u << 3,
        kHostError = 1u << 4,
    };

    struct HostState
    {
        std::uint32_t seq;              // seqlock: odd while writing
        std::uint32_t flags;
        std::uint32_t collisionEpoch;
        std::uint32_t toggleAck;
        std::uint64_t tick;

        float root[16];                  // column-major Skate root transform
        float velocity[3];

        float cameraPos[3];
        float cameraBasis[9];            // column-major 3x3
        float fovDeg;

        char state[32];                  // UTF-8, NUL-terminated/truncated
        std::uint8_t reserved[0xC0 - 0xB8];
    };
    static_assert(sizeof(HostState) == 0xC0);

    inline constexpr std::uint64_t kRingHeadOff = 0x00; // u64, Skyrim writes
    inline constexpr std::uint64_t kRingTailOff = 0x40; // u64, host writes
    inline constexpr std::uint64_t kRingDataOff = 0x80;
    inline constexpr std::uint64_t kRingDataBytes = kCollisionRingBytes - kRingDataOff;

    enum CollisionMessageType : std::uint32_t
    {
        kCollisionClear = 1,
        kCollisionTriangles = 2,
    };

    struct CollisionMessageHeader
    {
        std::uint32_t bytes;     // complete message bytes including this header
        std::uint32_t type;
        std::uint32_t epoch;
        std::uint32_t sequence;
    };
    static_assert(sizeof(CollisionMessageHeader) == 16);

    struct TriangleBatchHeader
    {
        std::int32_t regionX;
        std::int32_t regionY;
        std::int32_t regionZ;
        std::uint32_t count;
    };
    static_assert(sizeof(TriangleBatchHeader) == 16);

    // Exact Skyrim collision triangle, already converted to SkyCraft's
    // X-right/Y-up/Z-forward-ish metre-scale protocol space.
    struct Triangle
    {
        float v[9];
        std::uint32_t flags;
    };
    static_assert(sizeof(Triangle) == 40);
}
