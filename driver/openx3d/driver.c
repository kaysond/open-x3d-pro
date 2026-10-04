#include "driver.h"

/* X=512 Y=512 hat centred Rz=128: what GET_INPUT_REPORT returns before the first USB frame. */
static const UCHAR kNeutralRaw[X3D_RAW_SIZE] = { 0x00, 0x02, 0x88, 0x80, 0x00, 0x00, 0x00 };

NTSTATUS
DriverEntry(_In_ PDRIVER_OBJECT DriverObject, _In_ PUNICODE_STRING RegistryPath)
{
    WDF_DRIVER_CONFIG config;

    WDF_DRIVER_CONFIG_INIT(&config, EvtDeviceAdd);
    return WdfDriverCreate(DriverObject, RegistryPath, WDF_NO_OBJECT_ATTRIBUTES, &config, WDF_NO_HANDLE);
}

NTSTATUS
EvtDeviceAdd(_In_ WDFDRIVER Driver, _Inout_ PWDFDEVICE_INIT DeviceInit)
{
    WDF_PNPPOWER_EVENT_CALLBACKS pnp;
    WDF_OBJECT_ATTRIBUTES attributes;
    WDF_IO_QUEUE_CONFIG queueConfig;
    WDFDEVICE device;
    PDEVICE_CONTEXT ctx;
    X3D_CONFIG config;
    NTSTATUS status;

    UNREFERENCED_PARAMETER(Driver);

    /* mshidumdf/hidclass above us is the FDO and power policy owner. */
    WdfFdoInitSetFilter(DeviceInit);

    WDF_PNPPOWER_EVENT_CALLBACKS_INIT(&pnp);
    pnp.EvtDevicePrepareHardware = EvtDevicePrepareHardware;
    pnp.EvtDeviceD0Entry = EvtDeviceD0Entry;
    pnp.EvtDeviceD0Exit = EvtDeviceD0Exit;
    WdfDeviceInitSetPnpPowerEventCallbacks(DeviceInit, &pnp);

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, DEVICE_CONTEXT);
    status = WdfDeviceCreate(&DeviceInit, &attributes, &device);
    if (!NT_SUCCESS(status)) {
        X3D_LOG("WdfDeviceCreate failed");
        return status;
    }

    ctx = GetDeviceContext(device); /* zero-initialised by the framework */
    ctx->HidAttributes.Size = (ULONG)sizeof(HID_DEVICE_ATTRIBUTES);

    WDF_OBJECT_ATTRIBUTES_INIT(&attributes);
    attributes.ParentObject = device;
    status = WdfSpinLockCreate(&attributes, &ctx->Lock);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    status = WdfWaitLockCreate(&attributes, &ctx->SetFeatureLock);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchParallel);
    /* hidclass sends IRP_MJ_INTERNAL_DEVICE_CONTROL; mshidumdf re-issues it as DEVICE_CONTROL. */
    queueConfig.EvtIoDeviceControl = EvtIoDeviceControl;
    status = WdfIoQueueCreate(device, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, WDF_NO_HANDLE);
    if (!NT_SUCCESS(status)) {
        X3D_LOG("default queue creation failed");
        return status;
    }

    WDF_IO_QUEUE_CONFIG_INIT(&queueConfig, WdfIoQueueDispatchManual);
    status = WdfIoQueueCreate(device, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, &ctx->ReadQueue);
    if (!NT_SUCCESS(status)) {
        X3D_LOG("read queue creation failed");
        return status;
    }

    RtlCopyMemory(ctx->LastRaw, kNeutralRaw, sizeof(kNeutralRaw));
    if (NT_SUCCESS(RegistryLoadConfig(device, &config))) {
        DeviceApplyConfig(ctx, &config, TRUE);
    } else {
        pipeline_identity(&config);
        DeviceApplyConfig(ctx, &config, FALSE);
    }
    return STATUS_SUCCESS;
}
