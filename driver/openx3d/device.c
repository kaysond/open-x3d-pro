#include "driver.h"

VOID
DeviceApplyConfig(_In_ PDEVICE_CONTEXT Ctx, _In_ const X3D_CONFIG *Config, _In_ BOOLEAN Persisted)
{
    X3D_CONFIG identity;
    UCHAR flags = (UCHAR)(Persisted ? X3D_FLAG_FROM_REGISTRY : 0);

    pipeline_identity(&identity);
    if (RtlEqualMemory(&identity, Config, sizeof(identity))) {
        flags = (UCHAR)(flags | X3D_FLAG_IDENTITY);
    }

    WdfSpinLockAcquire(Ctx->Lock);
    RtlCopyMemory(&Ctx->Config, Config, sizeof(Ctx->Config));
    Ctx->Flags = flags;
    /* GET_INPUT_REPORT reflects the new config before the next USB frame */
    pipeline_process(&Ctx->Config, Ctx->LastRaw, &Ctx->LastJoy);
    WdfSpinLockRelease(Ctx->Lock);
}

VOID
DeviceOnUsbReport(_In_ PDEVICE_CONTEXT Ctx, _In_reads_(X3D_RAW_SIZE) const UCHAR *Raw)
{
    WdfSpinLockAcquire(Ctx->Lock);
    RtlCopyMemory(Ctx->LastRaw, Raw, X3D_RAW_SIZE);
    Ctx->Seq++;
    pipeline_process(&Ctx->Config, Raw, &Ctx->LastJoy);
    WdfSpinLockRelease(Ctx->Lock);

    DeviceCompletePendingReads(Ctx);
}

/* Copies input report 1 or 2 (with its ID byte) into Buffer; returns its length, 0 for other IDs. */
size_t
DeviceBuildInputReport(_In_ PDEVICE_CONTEXT Ctx, _In_ UCHAR ReportId,
                       _Out_writes_bytes_(sizeof(X3D_JOY_REPORT)) UCHAR *Buffer)
{
    size_t length = 0;

    WdfSpinLockAcquire(Ctx->Lock);
    if (ReportId == X3D_REPORT_ID_JOY) {
        RtlCopyMemory(Buffer, &Ctx->LastJoy, sizeof(X3D_JOY_REPORT));
        length = sizeof(X3D_JOY_REPORT);
    } else if (ReportId == X3D_REPORT_ID_VENDOR) {
        Buffer[0] = X3D_REPORT_ID_VENDOR;
        RtlCopyMemory(Buffer + 1, Ctx->LastRaw, X3D_RAW_SIZE);
        Buffer[8] = (UCHAR)(Ctx->Seq & 0xFF);
        Buffer[9] = (UCHAR)(Ctx->Seq >> 8);
        Buffer[10] = Ctx->Flags;
        length = X3D_VENDOR_REPORT_SIZE;
    }
    WdfSpinLockRelease(Ctx->Lock);
    return length;
}

static VOID
CompleteRead(_In_ PDEVICE_CONTEXT Ctx, _In_ WDFREQUEST Request, _In_ UCHAR ReportId)
{
    UCHAR report[sizeof(X3D_JOY_REPORT)];
    size_t length = DeviceBuildInputReport(Ctx, ReportId, report);

    WdfSpinLockAcquire(Ctx->Lock);
    Ctx->LastReadId = ReportId;
    if (ReportId == X3D_REPORT_ID_VENDOR) {
        Ctx->LastVendorTick = GetTickCount64();
    }
    WdfSpinLockRelease(Ctx->Lock);

    WdfRequestComplete(Request, RequestCopyFromBuffer(Request, report, length));
}

/*
 * READ_REPORT policy. hidclass keeps a few READ_REPORT requests pending for the
 * whole device and routes each completed report to a collection by its ID, so
 * every USB frame (100 Hz) completes up to two pending reads: report 1
 * (joystick) then report 2 (vendor). If only one read is pending it gets report
 * 1, unless the previous completion was report 1 and the vendor collection has
 * not been fed for X3D_VENDOR_MAX_AGE_MS; then report 2 goes first.
 */
VOID
DeviceCompletePendingReads(_In_ PDEVICE_CONTEXT Ctx)
{
    WDFREQUEST request;
    UCHAR first = X3D_REPORT_ID_JOY;
    ULONGLONG now = GetTickCount64();

    WdfSpinLockAcquire(Ctx->Lock);
    if (Ctx->LastReadId == X3D_REPORT_ID_JOY && now - Ctx->LastVendorTick >= X3D_VENDOR_MAX_AGE_MS) {
        first = X3D_REPORT_ID_VENDOR;
    }
    WdfSpinLockRelease(Ctx->Lock);

    if (!NT_SUCCESS(WdfIoQueueRetrieveNextRequest(Ctx->ReadQueue, &request))) {
        return;
    }
    CompleteRead(Ctx, request, first);

    if (NT_SUCCESS(WdfIoQueueRetrieveNextRequest(Ctx->ReadQueue, &request))) {
        CompleteRead(Ctx, request, (UCHAR)(first == X3D_REPORT_ID_JOY ? X3D_REPORT_ID_VENDOR : X3D_REPORT_ID_JOY));
    }
}
