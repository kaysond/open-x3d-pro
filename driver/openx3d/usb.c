/* USB side: configure the device and stream the interrupt-IN pipe. Never sends
 * class requests: the X3D Pro stalls GET_REPORT/GET_FEATURE (Linux LG_NOGET). */
#include "driver.h"

NTSTATUS
EvtDevicePrepareHardware(_In_ WDFDEVICE Device, _In_ WDFCMRESLIST ResourcesRaw,
                         _In_ WDFCMRESLIST ResourcesTranslated)
{
    PDEVICE_CONTEXT ctx = GetDeviceContext(Device);
    WDF_USB_DEVICE_SELECT_CONFIG_PARAMS selectParams;
    WDF_USB_CONTINUOUS_READER_CONFIG readerConfig;
    WDF_USB_PIPE_INFORMATION pipeInfo;
    USB_DEVICE_DESCRIPTOR deviceDescriptor;
    WDFUSBINTERFACE usbInterface;
    ULONG maxPacket = 0;
    UCHAR pipeCount;
    UCHAR i;
    NTSTATUS status;

    UNREFERENCED_PARAMETER(ResourcesRaw);
    UNREFERENCED_PARAMETER(ResourcesTranslated);

    /* Created once; PrepareHardware runs again after a PnP stop/start. */
    if (ctx->UsbDevice == NULL) {
        status = WdfUsbTargetDeviceCreate(Device, WDF_NO_OBJECT_ATTRIBUTES, &ctx->UsbDevice);
        if (!NT_SUCCESS(status)) {
            X3D_LOG("WdfUsbTargetDeviceCreate failed");
            return status;
        }
    }

    /* hidclass derives the HID hardware IDs from these, so keep the real VID/PID/REV. */
    WdfUsbTargetDeviceGetDeviceDescriptor(ctx->UsbDevice, &deviceDescriptor);
    ctx->HidAttributes.VendorID = deviceDescriptor.idVendor;
    ctx->HidAttributes.ProductID = deviceDescriptor.idProduct;
    ctx->HidAttributes.VersionNumber = deviceDescriptor.bcdDevice;

    WDF_USB_DEVICE_SELECT_CONFIG_PARAMS_INIT_SINGLE_INTERFACE(&selectParams);
    status = WdfUsbTargetDeviceSelectConfig(ctx->UsbDevice, WDF_NO_OBJECT_ATTRIBUTES, &selectParams);
    if (!NT_SUCCESS(status)) {
        X3D_LOG("WdfUsbTargetDeviceSelectConfig failed");
        return status;
    }
    usbInterface = selectParams.Types.SingleInterface.ConfiguredUsbInterface;

    ctx->InterruptPipe = NULL;
    pipeCount = WdfUsbInterfaceGetNumConfiguredPipes(usbInterface);
    for (i = 0; i < pipeCount; i++) {
        WDFUSBPIPE pipe;

        WDF_USB_PIPE_INFORMATION_INIT(&pipeInfo);
        pipe = WdfUsbInterfaceGetConfiguredPipe(usbInterface, i, &pipeInfo);
        if (pipeInfo.PipeType == WdfUsbPipeTypeInterrupt && WdfUsbTargetPipeIsInEndpoint(pipe)) {
            ctx->InterruptPipe = pipe; /* endpoint 0x81 */
            maxPacket = pipeInfo.MaximumPacketSize;
            break;
        }
    }
    if (ctx->InterruptPipe == NULL || maxPacket == 0) {
        X3D_LOG("no interrupt-IN pipe");
        return STATUS_INVALID_DEVICE_STATE;
    }

    /* wMaxPacketSize is 7: allow reads that are not a multiple of the packet size. */
    WdfUsbTargetPipeSetNoMaximumPacketSizeCheck(ctx->InterruptPipe);

    WDF_USB_CONTINUOUS_READER_CONFIG_INIT(&readerConfig, EvtUsbReadComplete, ctx, maxPacket);
    readerConfig.EvtUsbTargetPipeReadersFailed = EvtUsbReadersFailed;
    /* Reader starts in D0Entry via WdfIoTargetStart. */
    status = WdfUsbTargetPipeConfigContinuousReader(ctx->InterruptPipe, &readerConfig);
    if (!NT_SUCCESS(status)) {
        X3D_LOG("WdfUsbTargetPipeConfigContinuousReader failed");
    }
    return status;
}

NTSTATUS
EvtDeviceD0Entry(_In_ WDFDEVICE Device, _In_ WDF_POWER_DEVICE_STATE PreviousState)
{
    PDEVICE_CONTEXT ctx = GetDeviceContext(Device);

    UNREFERENCED_PARAMETER(PreviousState);
    return WdfIoTargetStart(WdfUsbTargetPipeGetIoTarget(ctx->InterruptPipe));
}

NTSTATUS
EvtDeviceD0Exit(_In_ WDFDEVICE Device, _In_ WDF_POWER_DEVICE_STATE TargetState)
{
    PDEVICE_CONTEXT ctx = GetDeviceContext(Device);

    UNREFERENCED_PARAMETER(TargetState);
    WdfIoTargetStop(WdfUsbTargetPipeGetIoTarget(ctx->InterruptPipe), WdfIoTargetCancelSentIo);
    return STATUS_SUCCESS;
}

/* May run concurrently on several threads (the framework posts 2 readers). */
VOID
EvtUsbReadComplete(_In_ WDFUSBPIPE Pipe, _In_ WDFMEMORY Buffer, _In_ size_t NumBytesTransferred,
                   _In_ WDFCONTEXT Context)
{
    UNREFERENCED_PARAMETER(Pipe);

    if (NumBytesTransferred < X3D_RAW_SIZE) {
        return;
    }
    DeviceOnUsbReport((PDEVICE_CONTEXT)Context, (const UCHAR *)WdfMemoryGetBuffer(Buffer, NULL));
}

BOOLEAN
EvtUsbReadersFailed(_In_ WDFUSBPIPE Pipe, _In_ NTSTATUS Status, _In_ USBD_STATUS UsbdStatus)
{
    UNREFERENCED_PARAMETER(Pipe);
    UNREFERENCED_PARAMETER(Status);
    UNREFERENCED_PARAMETER(UsbdStatus);

    X3D_LOG("continuous reader failed; framework resets the pipe and restarts it");
    return TRUE;
}
