/* HID minidriver IOCTLs as delivered by mshidumdf. Buffer handling follows
 * Windows-driver-samples hid/vhidmini2 (UMDF 2 build). Every path completes
 * the request exactly once, except READ_REPORT which is parked in ReadQueue. */
#include "driver.h"
#include "descriptor.h"

static const WCHAR kManufacturer[] = L"Logitech";
static const WCHAR kProduct[] = L"Logitech Extreme 3D Pro (Open X3D)";

NTSTATUS
RequestCopyFromBuffer(_In_ WDFREQUEST Request, _In_reads_bytes_(Length) const VOID *Source, _In_ size_t Length)
{
    WDFMEMORY memory;
    size_t outputLength;
    NTSTATUS status;

    status = WdfRequestRetrieveOutputMemory(Request, &memory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    WdfMemoryGetBuffer(memory, &outputLength);
    if (outputLength < Length) {
        return STATUS_INVALID_BUFFER_SIZE;
    }
    status = WdfMemoryCopyFromBuffer(memory, 0, (PVOID)Source, Length);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    WdfRequestSetInformation(Request, Length);
    return STATUS_SUCCESS;
}

/*
 * mshidumdf cannot marshal HID_XFER_PACKET's embedded pointer, so the IOCTL_UMDF_HID_*
 * variants carry it in pieces (vhidmini2 util.c):
 *   to read from device: report buffer = output buffer, report ID = first input byte
 *   to write to device:  report buffer = input buffer,  report ID = output buffer *length*
 */
static NTSTATUS
GetXferPacketToRead(_In_ WDFREQUEST Request, _Out_ HID_XFER_PACKET *Packet)
{
    WDFMEMORY inputMemory, outputMemory;
    size_t inputLength, outputLength;
    PVOID inputBuffer, outputBuffer;
    NTSTATUS status;

    status = WdfRequestRetrieveInputMemory(Request, &inputMemory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    inputBuffer = WdfMemoryGetBuffer(inputMemory, &inputLength);
    if (inputLength < sizeof(UCHAR)) {
        return STATUS_INVALID_BUFFER_SIZE;
    }
    Packet->reportId = *(PUCHAR)inputBuffer;

    status = WdfRequestRetrieveOutputMemory(Request, &outputMemory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    outputBuffer = WdfMemoryGetBuffer(outputMemory, &outputLength);
    Packet->reportBuffer = (PUCHAR)outputBuffer;
    Packet->reportBufferLen = (ULONG)outputLength;
    return STATUS_SUCCESS;
}

static NTSTATUS
GetXferPacketToWrite(_In_ WDFREQUEST Request, _Out_ HID_XFER_PACKET *Packet)
{
    WDFMEMORY inputMemory, outputMemory;
    size_t inputLength, outputLength;
    PVOID inputBuffer;
    NTSTATUS status;

    status = WdfRequestRetrieveOutputMemory(Request, &outputMemory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    WdfMemoryGetBuffer(outputMemory, &outputLength); /* never read the output buffer itself */
    Packet->reportId = (UCHAR)outputLength;

    status = WdfRequestRetrieveInputMemory(Request, &inputMemory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    inputBuffer = WdfMemoryGetBuffer(inputMemory, &inputLength);
    Packet->reportBuffer = (PUCHAR)inputBuffer;
    Packet->reportBufferLen = (ULONG)inputLength;
    return STATUS_SUCCESS;
}

static VOID
Put16(_Out_writes_bytes_(2) PUCHAR p, _In_ ULONG v)
{
    p[0] = (UCHAR)(v & 0xFF);
    p[1] = (UCHAR)((v >> 8) & 0xFF);
}

static NTSTATUS
GetInputReport(_In_ PDEVICE_CONTEXT Ctx, _In_ WDFREQUEST Request)
{
    HID_XFER_PACKET packet;
    UCHAR report[sizeof(X3D_JOY_REPORT)];
    size_t length;
    NTSTATUS status;

    status = GetXferPacketToRead(Request, &packet);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    length = DeviceBuildInputReport(Ctx, packet.reportId, report);
    if (length == 0) {
        return STATUS_INVALID_PARAMETER;
    }
    if (packet.reportBufferLen < length) {
        return STATUS_INVALID_BUFFER_SIZE;
    }
    RtlCopyMemory(packet.reportBuffer, report, length);
    WdfRequestSetInformation(Request, length);
    return STATUS_SUCCESS;
}

static NTSTATUS
GetFeature(_In_ PDEVICE_CONTEXT Ctx, _In_ WDFREQUEST Request)
{
    HID_XFER_PACKET packet;
    ULONG size;
    PUCHAR b;
    NTSTATUS status;

    status = GetXferPacketToRead(Request, &packet);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    if (packet.reportId == X3D_REPORT_ID_CONFIG) {
        size = X3D_CONFIG_REPORT_SIZE;
    } else if (packet.reportId == X3D_REPORT_ID_INFO) {
        size = X3D_INFO_REPORT_SIZE;
    } else {
        return STATUS_INVALID_PARAMETER;
    }
    if (packet.reportBufferLen < size) {
        return STATUS_INVALID_BUFFER_SIZE;
    }

    b = packet.reportBuffer;
    RtlZeroMemory(b, size);
    b[0] = packet.reportId;
    WdfSpinLockAcquire(Ctx->Lock);
    if (packet.reportId == X3D_REPORT_ID_CONFIG) {
        RtlCopyMemory(b + 1, &Ctx->Config, sizeof(X3D_CONFIG));
    } else {
        Put16(b + 1, X3D_DRIVER_VERSION_BCD);
        Put16(b + 3, Ctx->HidAttributes.VersionNumber);
        Put16(b + 5, Ctx->Config.crc32 & 0xFFFF);
        Put16(b + 7, Ctx->Config.crc32 >> 16); /* u32 LE */
        RtlCopyMemory(b + 9, Ctx->LastRaw, X3D_RAW_SIZE);
        b[16] = Ctx->Flags;
    }
    WdfSpinLockRelease(Ctx->Lock);

    WdfRequestSetInformation(Request, size);
    return STATUS_SUCCESS;
}

static NTSTATUS
SetFeature(_In_ WDFDEVICE Device, _In_ PDEVICE_CONTEXT Ctx, _In_ WDFREQUEST Request)
{
    HID_XFER_PACKET packet;
    X3D_CONFIG config;
    BOOLEAN persisted;
    NTSTATUS status;

    status = GetXferPacketToWrite(Request, &packet);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    /* reportBuffer[0] is the report ID, the blob follows */
    if (packet.reportId != X3D_REPORT_ID_CONFIG || packet.reportBufferLen < 1 + sizeof(X3D_CONFIG) ||
        !pipeline_validate(packet.reportBuffer + 1, packet.reportBufferLen - 1)) {
        return STATUS_INVALID_PARAMETER;
    }
    RtlCopyMemory(&config, packet.reportBuffer + 1, sizeof(config));

    /* One writer at a time, so the registry always holds the active config. A failed
     * registry write still applies the config; the app sees it as flag bit 0 clear. */
    WdfWaitLockAcquire(Ctx->SetFeatureLock, NULL);
    persisted = (BOOLEAN)NT_SUCCESS(RegistrySaveConfig(Device, &config));
    DeviceApplyConfig(Ctx, &config, persisted);
    WdfWaitLockRelease(Ctx->SetFeatureLock);

    WdfRequestSetInformation(Request, packet.reportBufferLen);
    return STATUS_SUCCESS;
}

static NTSTATUS
GetStringId(_In_ WDFREQUEST Request, _Out_ ULONG *StringId)
{
    WDFMEMORY inputMemory;
    size_t inputLength;
    PVOID inputBuffer;
    NTSTATUS status;

    /* mshidumdf passes the ULONG (low word id/index, high word LANGID) as the input buffer */
    status = WdfRequestRetrieveInputMemory(Request, &inputMemory);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    inputBuffer = WdfMemoryGetBuffer(inputMemory, &inputLength);
    if (inputLength < sizeof(ULONG)) {
        return STATUS_INVALID_BUFFER_SIZE;
    }
    *StringId = *(PULONG)inputBuffer & 0xFFFF;
    return STATUS_SUCCESS;
}

static NTSTATUS
GetString(_In_ WDFREQUEST Request, _In_ BOOLEAN Indexed)
{
    ULONG id;
    NTSTATUS status = GetStringId(Request, &id);

    if (!NT_SUCCESS(status)) {
        return status;
    }
    /* Indexed strings use the USB string indexes: iManufacturer 1, iProduct 2, no serial. */
    if (Indexed ? id == 1 : id == HID_STRING_ID_IMANUFACTURER) {
        return RequestCopyFromBuffer(Request, kManufacturer, sizeof(kManufacturer));
    }
    if (Indexed ? id == 2 : id == HID_STRING_ID_IPRODUCT) {
        return RequestCopyFromBuffer(Request, kProduct, sizeof(kProduct));
    }
    return STATUS_INVALID_PARAMETER;
}

VOID
EvtIoDeviceControl(_In_ WDFQUEUE Queue, _In_ WDFREQUEST Request, _In_ size_t OutputBufferLength,
                   _In_ size_t InputBufferLength, _In_ ULONG IoControlCode)
{
    WDFDEVICE device = WdfIoQueueGetDevice(Queue);
    PDEVICE_CONTEXT ctx = GetDeviceContext(device);
    NTSTATUS status;

    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    switch (IoControlCode) {
    case IOCTL_HID_GET_DEVICE_DESCRIPTOR:
        status = RequestCopyFromBuffer(Request, &G_HidDescriptor, G_HidDescriptor.bLength);
        break;
    case IOCTL_HID_GET_DEVICE_ATTRIBUTES:
        status = RequestCopyFromBuffer(Request, &ctx->HidAttributes, sizeof(HID_DEVICE_ATTRIBUTES));
        break;
    case IOCTL_HID_GET_REPORT_DESCRIPTOR:
        status = RequestCopyFromBuffer(Request, G_ReportDescriptor, sizeof(G_ReportDescriptor));
        break;
    case IOCTL_HID_READ_REPORT:
        /* completed from the USB reader, see DeviceCompletePendingReads */
        status = WdfRequestForwardToIoQueue(Request, ctx->ReadQueue);
        if (NT_SUCCESS(status)) {
            return;
        }
        break;
    case IOCTL_UMDF_HID_GET_INPUT_REPORT:
        status = GetInputReport(ctx, Request);
        break;
    case IOCTL_UMDF_HID_GET_FEATURE:
        status = GetFeature(ctx, Request);
        break;
    case IOCTL_UMDF_HID_SET_FEATURE:
        status = SetFeature(device, ctx, Request);
        break;
    case IOCTL_HID_GET_STRING:
        status = GetString(Request, FALSE);
        break;
    case IOCTL_HID_GET_INDEXED_STRING:
        status = GetString(Request, TRUE);
        break;
    case IOCTL_HID_ACTIVATE_DEVICE:
    case IOCTL_HID_DEACTIVATE_DEVICE:
        status = STATUS_SUCCESS;
        break;
    case IOCTL_HID_WRITE_REPORT:
    case IOCTL_UMDF_HID_SET_OUTPUT_REPORT:
        status = STATUS_NOT_SUPPORTED; /* no output reports */
        break;
    case IOCTL_HID_SEND_IDLE_NOTIFICATION_REQUEST:
        /* Not success: we never call the idle callback, so claiming success could
         * leave hidclass waiting. vhidmini2 and DMF VirtualHidMini do the same. */
    default:
        status = STATUS_NOT_IMPLEMENTED;
        break;
    }
    WdfRequestComplete(Request, status);
}
