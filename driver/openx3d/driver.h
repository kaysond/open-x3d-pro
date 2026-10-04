/* Open X3D Pro UMDF 2 HID minidriver: shared includes and entry points.
 * Stack: hidclass+mshidumdf (FDO) -> WUDFRd -> this driver (UMDF filter) -> USB PDO. */
#pragma once

#include <windows.h>
#include <wdf.h>
#include <usb.h>
#include <wdfusb.h>
#include <hidport.h>

#include "pipeline.h"
#include "device.h"

#if defined(DBG) && DBG
#define X3D_LOG(msg) OutputDebugStringA("openx3d: " msg "\n")
#else
#define X3D_LOG(msg) ((void)0)
#endif

DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD EvtDeviceAdd;

/* usb.c */
EVT_WDF_DEVICE_PREPARE_HARDWARE EvtDevicePrepareHardware;
EVT_WDF_DEVICE_D0_ENTRY EvtDeviceD0Entry;
EVT_WDF_DEVICE_D0_EXIT EvtDeviceD0Exit;
EVT_WDF_USB_READER_COMPLETION_ROUTINE EvtUsbReadComplete;
EVT_WDF_USB_READERS_FAILED EvtUsbReadersFailed;

/* hid.c */
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL EvtIoDeviceControl;
NTSTATUS RequestCopyFromBuffer(_In_ WDFREQUEST Request, _In_reads_bytes_(Length) const VOID *Source, _In_ size_t Length);

/* registry.c */
NTSTATUS RegistryLoadConfig(_In_ WDFDEVICE Device, _Out_ X3D_CONFIG *Config);
NTSTATUS RegistrySaveConfig(_In_ WDFDEVICE Device, _In_ const X3D_CONFIG *Config);
