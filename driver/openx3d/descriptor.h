/* HID report descriptor published by openx3d (CONTRACT 2.1). Byte-identical to
 * x3d_core::hid::REPORT_DESCRIPTOR in core/src/hid.rs; `make -C driver/test`
 * diffs the two. Included by hid.c only. */
#pragma once

static const UCHAR G_ReportDescriptor[] = {
    /* Collection 1: joystick, report ID 1, 14 bytes incl. ID */
    0x05, 0x01,                   /* Usage Page (Generic Desktop) */
    0x09, 0x04,                   /* Usage (Joystick) */
    0xA1, 0x01,                   /* Collection (Application) */
    0x85, 0x01,                   /*   Report ID (1) */
    0x09, 0x30,                   /*   Usage (X) */
    0x09, 0x31,                   /*   Usage (Y) */
    0x09, 0x35,                   /*   Usage (Rz) */
    0x09, 0x36,                   /*   Usage (Slider) */
    0x15, 0x00,                   /*   Logical Minimum (0) */
    0x27, 0xFF, 0xFF, 0x00, 0x00, /*   Logical Maximum (65535); 4-byte form, 2-byte would be -1 */
    0x75, 0x10,                   /*   Report Size (16) */
    0x95, 0x04,                   /*   Report Count (4) */
    0x81, 0x02,                   /*   Input (Data,Var,Abs) */
    0x09, 0x39,                   /*   Usage (Hat switch) */
    0x25, 0x07,                   /*   Logical Maximum (7) */
    0x35, 0x00,                   /*   Physical Minimum (0) */
    0x46, 0x3B, 0x01,             /*   Physical Maximum (315) */
    0x65, 0x14,                   /*   Unit (Eng Rot: Degrees) */
    0x75, 0x04,                   /*   Report Size (4) */
    0x95, 0x01,                   /*   Report Count (1) */
    0x81, 0x42,                   /*   Input (Data,Var,Abs,Null State): 8 = centred */
    0x45, 0x00,                   /*   Physical Maximum (0): physical = logical again */
    0x65, 0x00,                   /*   Unit (None) */
    0x81, 0x03,                   /*   Input (Const,Var,Abs): 4-bit pad */
    0x05, 0x09,                   /*   Usage Page (Button) */
    0x19, 0x01,                   /*   Usage Minimum (1) */
    0x29, 0x20,                   /*   Usage Maximum (32) */
    0x25, 0x01,                   /*   Logical Maximum (1) */
    0x75, 0x01,                   /*   Report Size (1) */
    0x95, 0x20,                   /*   Report Count (32) */
    0x81, 0x02,                   /*   Input (Data,Var,Abs) */
    0xC0,                         /* End Collection */
    /* Collection 2: vendor channel for the app */
    0x06, 0x00, 0xFF,             /* Usage Page (Vendor 0xFF00) */
    0x09, 0x01,                   /* Usage (0x01) */
    0xA1, 0x01,                   /* Collection (Application) */
    0x85, 0x02,                   /*   Report ID (2) */
    0x09, 0x01,                   /*   Usage (0x01) */
    0x15, 0x00,                   /*   Logical Minimum (0) */
    0x26, 0xFF, 0x00,             /*   Logical Maximum (255) */
    0x75, 0x08,                   /*   Report Size (8) */
    0x95, 0x0A,                   /*   Report Count (10) */
    0x81, 0x02,                   /*   Input (Data,Var,Abs): raw[7], seq u16, flags u8 */
    0x85, 0x03,                   /*   Report ID (3) */
    0x09, 0x02,                   /*   Usage (0x02) */
    0x95, 0xFF,                   /*   Report Count (255) */
    0xB1, 0x02,                   /*   Feature (Data,Var,Abs): config blob, zero padded */
    0x85, 0x04,                   /*   Report ID (4) */
    0x09, 0x03,                   /*   Usage (0x03) */
    0x95, 0x1F,                   /*   Report Count (31) */
    0xB1, 0x02,                   /*   Feature (Data,Var,Abs): driver info, GET only */
    0xC0,                         /* End Collection */
};
C_ASSERT(sizeof(G_ReportDescriptor) == 106);

static const HID_DESCRIPTOR G_HidDescriptor = {
    0x09,                         /* bLength */
    0x21,                         /* bDescriptorType: HID */
    0x0110,                       /* bcdHID 1.10, as the device reports */
    0x00,                         /* bCountry: not localized */
    0x01,                         /* bNumDescriptors */
    { { 0x22, (USHORT)sizeof(G_ReportDescriptor) } }, /* report descriptor */
};
