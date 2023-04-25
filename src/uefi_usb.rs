use core::convert::TryFrom;
use core::mem::MaybeUninit;
use log::{debug, error, info};
use uefi::prelude::*;
use uefi::proto::unsafe_protocol;
use uefi::table::boot::ScopedProtocol;
use uefi::{Char16, Result};

use crate::TIMEOUT;
use crate::VERBOSE;

pub const HID_ELEMENT_TAG_LONG: u8 = 15;

const USE_CONTROL_TRANSFER: bool = false;

pub enum EndpointTypes {
    Control = 0x00,
    Isochronous = 0x01,
    Bulk = 0x02,
    Interrupt = 0x03,
}

#[repr(u32)]
#[derive(Debug)]
pub enum UsbTransferResult {
    NoError = 0x00,
    NotExecute = 0x01,
    Stall = 0x02,
    Buffer = 0x04,
    Babble = 0x08,
    Nak = 0x10,
    Crc = 0x20,
    Timeout = 0x40,
    Bitstuff = 0x80,
    System = 0x100,
}

impl TryFrom<u32> for UsbTransferResult {
    type Error = ();

    fn try_from(v: u32) -> core::result::Result<Self, Self::Error> {
        match v {
            x if x == UsbTransferResult::NoError as u32 => Ok(UsbTransferResult::NoError),
            x if x == UsbTransferResult::NotExecute as u32 => Ok(UsbTransferResult::NotExecute),
            x if x == UsbTransferResult::Stall as u32 => Ok(UsbTransferResult::Stall),
            x if x == UsbTransferResult::Buffer as u32 => Ok(UsbTransferResult::Buffer),
            x if x == UsbTransferResult::Babble as u32 => Ok(UsbTransferResult::Babble),
            x if x == UsbTransferResult::Nak as u32 => Ok(UsbTransferResult::Nak),
            x if x == UsbTransferResult::Crc as u32 => Ok(UsbTransferResult::Crc),
            x if x == UsbTransferResult::Timeout as u32 => Ok(UsbTransferResult::Timeout),
            x if x == UsbTransferResult::Bitstuff as u32 => Ok(UsbTransferResult::Bitstuff),
            x if x == UsbTransferResult::System as u32 => Ok(UsbTransferResult::System),
            _ => core::result::Result::Err(()),
        }
    }
}

pub struct HidDevice<'a> {
    pub usb_io: ScopedProtocol<'a, UsbIo>,
    pub interface_no: u8,
    pub out_endpoint_addr: u8,
    pub in_endpoint_addr: u8,
}

impl HidDevice<'_> {
    // TODO: data shouldn't need to be mut here
    pub fn write(&mut self, data: &mut [u8]) -> Result<usize> {
        assert!(!data.is_empty());
        set_report_request(
            &mut self.usb_io,
            self.interface_no,
            data[0],
            ReportType::Output,
            &mut data[1..],
            self.out_endpoint_addr,
        )
        .unwrap();
        Ok(0) // TODO: Return proper value
    }
    pub fn read(&mut self, data: &mut [u8]) -> Result<usize> {
        assert!(!data.is_empty());
        get_report_request(
            &mut self.usb_io,
            self.interface_no,
            data[0],
            ReportType::Input,
            //&mut data[1..],
            data,
            self.in_endpoint_addr,
        )
        .unwrap();
        Ok(0) // TODO: Return proper value
    }
    // TODO: Implement feature set/get report
    pub fn set_blocking_mode(&self, _: bool) -> Result<()> {
        // Just to be compatible with hidapi
        Ok(())
    }
}

#[repr(u8)]
pub enum ReportType {
    Input = 1,
    Output = 2,
    Feature = 3,
}

pub enum UsagePage {
    GenericDesktop = 0x01,
    Keyboard = 0x07,
    Led = 0x08,
    Button = 0x09,
    Consumer = 0x0C,
    Sensors = 0x20,
    LightingAndIllumination = 0x59,
    QmkConsole = 0xFF31,
    Raw = 0xFF60,
}

#[repr(u16)]
pub enum ReportFormat {
    Short = 0x00,
    Long = 0x01,
}

#[repr(C)]
pub enum UsbDataDirection {
    DataIn = 0x00,
    DataOut = 0x01,
    NoData = 0x02,
}

#[repr(u8)]
pub enum UsbDescriptorType {
    Device = 0x01,
    Config = 0x02,
    String = 0x03,
    Interface = 0x04,
    Endpoint = 0x05,
    Hid = 0x21,
    Report = 0x22,
}

#[repr(C, packed)]
pub struct HidDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub bcd_hid: u16,
    pub country_code: u8,
    pub num_descriptors: u8,
    pub hid_class_desc: [HidClassDescriptor; 1],
}

#[repr(C, packed)]
pub struct HidClassDescriptor {
    pub descriptor_type: u8,
    pub descriptor_length: u16,
}

//#[derive(Eq, Pointee)]
#[repr(C, packed)]
pub struct UsbDeviceRequest {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

//#[derive(Eq, Pointee)]
#[repr(C, packed)]
pub struct UsbDeviceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub bcd_usb: u16,
    pub device_class: u8,
    pub device_sub_class: u8,
    pub device_protocol: u8,
    pub max_packet_size0: u8,
    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,
    pub str_manufacturer: u8,
    pub str_product: u8,
    pub str_serial_number: u8,
    pub num_configurations: u8,
}

#[repr(C, packed)]
pub struct UsbConfigDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub total_length: u16,
    pub num_interfaces: u8,
    pub configuration_value: u8,
    pub configuration: u8,
    pub attributes: u8,
    pub max_power: u8,
}

#[repr(C, packed)]
pub struct UsbInterfaceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub num_endpoints: u8,
    pub interface_class: u8,
    pub interface_sub_class: u8,
    pub interface_protocol: u8,
    pub interface: u8,
}

#[repr(C, packed)]
pub struct UsbEndpointDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub endpoint_address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}

impl UsbEndpointDescriptor {
    pub fn is_in(&self) -> bool {
        (self.endpoint_address & 0x80) == 0x80
    }
    // TODO: Generalize with all types, not just interrupt
    pub fn is_interrupt(&self) -> bool {
        (self.attributes & 0b11) == 0b11
    }
}

#[repr(C, packed)]
pub struct UsbDescriptorHead {
    pub len: u8,
    pub desc_type: u8,
}

#[derive(Debug)]
pub struct HidElement {
    _format: u16, // TODO: I don't think it needs to be u16. u8 should be fine
    _size: u8,
    tag: HidTypeTag,
    /// Can be anything from u8 to *const u8
    data: Option<HidData>,
}

#[derive(Debug)]
pub enum MainTag {
    Input = 8,
    Output = 9,
    Feature = 10,
    Collection = 11,
    EndCollection = 12,
}

#[derive(Debug)]
pub enum GlobalTag {
    UsagePage = 0,
    LogicalMinimum = 1,
    LogicalMaximum = 2,
    PhysicalMinimum = 3,
    PhysicalMaximum = 4,
    UnitExponent = 5,
    Unit = 6,
    ReportSize = 7,
    ReportId = 8,
    ReportCount = 9,
    Push = 10,
    Pop = 11,
}

#[derive(Debug)]
pub enum LocalTag {
    Usage = 0,
    UsageMinimum = 1,
    UsageMaximum = 2,
    DesignatorIndex = 3,
    DesignatorMinimum = 4,
    DesignatorMaximum = 5,
    StringIndex = 7,
    StringMinimum = 8,
    StringMaximum = 9,
    Delimiter = 10,
}

#[derive(Debug)]
pub enum HidTypeTag {
    Main(MainTag),
    Global(GlobalTag),
    Local(LocalTag),
    Reserved,
}

#[derive(Debug)]
pub enum HidData {
    Uint8(u8),
    Uint16(u16),
    Uint32(u32),
    Int8(i8),
    Int16(i16),
    Int32(i32),
    LongData(*const u8),
}

/// Foo
#[repr(C)]
#[unsafe_protocol("2b2f68d6-0cd2-44cf-8e8b-bba20b1b5b75")]
pub struct UsbIo {
    usb_control_transfer: extern "efiapi" fn(
        &mut UsbIo,
        &mut UsbDeviceRequest,
        UsbDataDirection,
        u32,
        *mut u8,
        usize,
        *mut u32,
    ) -> Status,

    usb_bulk_transfer:
        extern "efiapi" fn(&mut UsbIo, u8, *mut u8, *mut usize, usize, u32) -> Status,
    // TODO: usb_async_interrupt_transfer
    usb_async_interrupt_transfer: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_sync_interrupt_transfer:
        extern "efiapi" fn(&mut UsbIo, u8, *mut u8, *mut usize, usize, *mut u32) -> Status,
    usb_isochronous_transfer:
        extern "efiapi" fn(&mut UsbIo, u8, *mut u8, usize, *mut u32) -> Status,
    // TODO: async_isochronous_transfer
    usb_async_isochronous_transfer: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_get_device_descriptor: extern "efiapi" fn(&mut UsbIo, *mut UsbDeviceDescriptor) -> Status,
    usb_get_config_descriptor: extern "efiapi" fn(&mut UsbIo, *mut UsbConfigDescriptor) -> Status,
    usb_get_interface_descriptor:
        extern "efiapi" fn(&mut UsbIo, *mut UsbInterfaceDescriptor) -> Status,
    usb_get_endpoint_descriptor:
        extern "efiapi" fn(&mut UsbIo, u8, *mut UsbEndpointDescriptor) -> Status,
    usb_get_string_descriptor: extern "efiapi" fn(&mut UsbIo, u16, u8, *mut *mut Char16) -> Status,
    usb_get_supported_languages: extern "efiapi" fn(&mut UsbIo, *mut *mut u16, *mut u16) -> Status,
    usb_port_reset: extern "efiapi" fn(&mut UsbIo) -> Status,
}
impl UsbIo {
    pub fn sync_interrupt_transfer(
        &mut self,
        endpoint: u8,
        timeout: u32,
        data: &mut [u8],
    ) -> Result<(), usize> {
        let mut buffer_size = data.len();
        let mut transfer_result = 0; // TODO: Use this value?
        (self.usb_sync_interrupt_transfer)(
            self,
            endpoint,
            data.as_mut_ptr(),
            &mut buffer_size,
            timeout as usize,
            &mut transfer_result,
        )
        .into_with(
            || {
                if buffer_size != data.len() {
                    error!(
                        "!!!!sync_interrupt_transfer: buffer_size != data.len(), {}, {}",
                        buffer_size,
                        data.len()
                    )
                }
            }, //debug_assert_eq!(buffer_size, data.len()),
            |_| {
                error!(
                    "Interrupt transfer failed with: {:?}",
                    UsbTransferResult::try_from(transfer_result).unwrap()
                );
                buffer_size
            },
        )
    }

    pub fn control_transfer(
        &mut self,
        request: &mut UsbDeviceRequest,
        direction: UsbDataDirection,
        timeout: u32,
        data: &mut [u8],
    ) -> Result<(), usize> {
        let buffer_size = data.len();
        let mut transfer_result = 0; // TODO: Use this value?
        (self.usb_control_transfer)(
            self,
            request,
            direction,
            timeout,
            data.as_mut_ptr(),
            buffer_size,
            &mut transfer_result,
        )
        .into_with(
            || debug_assert_eq!(buffer_size, data.len()),
            |_| {
                error!("control_transfer failed with: {:?}",
                    UsbTransferResult::try_from(transfer_result).unwrap()
                );
                buffer_size
            },
        )
    }

    //pub fn get_string_descriptor(&mut self, lang_id: u16, string_id: u8) -> Result<CStr16> {
    //    (self.get_string_descriptor)(self, lang_id, string_id, )
    //}

    pub fn get_device_descriptor(&mut self) -> Result<UsbDeviceDescriptor> {
        let mut desc = MaybeUninit::<UsbDeviceDescriptor>::uninit();
        (self.usb_get_device_descriptor)(self, desc.as_mut_ptr())
            .into_with_val(|| unsafe { desc.assume_init() })
    }

    pub fn get_interface_descriptor(&mut self) -> Result<UsbInterfaceDescriptor> {
        let mut desc = MaybeUninit::<UsbInterfaceDescriptor>::uninit();
        (self.usb_get_interface_descriptor)(self, desc.as_mut_ptr())
            .into_with_val(|| unsafe { desc.assume_init() })
    }

    pub fn get_config_descriptor(&mut self) -> Result<UsbConfigDescriptor> {
        let mut desc = MaybeUninit::<UsbConfigDescriptor>::uninit();
        (self.usb_get_config_descriptor)(self, desc.as_mut_ptr())
            .into_with_val(|| unsafe { desc.assume_init() })
    }

    pub fn get_endpoint_descriptor(&mut self, index: u8) -> Result<UsbEndpointDescriptor> {
        let mut desc = MaybeUninit::<UsbEndpointDescriptor>::uninit();
        (self.usb_get_endpoint_descriptor)(self, index, desc.as_mut_ptr())
            .into_with_val(|| unsafe { desc.assume_init() })
    }
}

#[repr(u8)]
pub enum UsbRequestType {
    HidGetRequest = 0xA1,
    HidSetRequest = 0x21,
    GetDescriptor = 0x80,
}

pub enum HidRequest {
    GetReportRequest = 0x01,
    GetIdleRequest = 0x02,
    GetProtocolRequest = 0x03,
    SetReportRequest = 0x09,
    SetIdleRequest = 0x0a,
    SetProtocolRequest = 0x0b,
}

pub enum DeviceRequest {
    UsbGetDescriptor = 0x06,
    UsbSetDescriptor = 0x07,
}

fn set_report_request(
    usb_io: &mut UsbIo,
    _interface: u8,
    _report_id: u8,
    _report_type: ReportType,
    report: &mut [u8],
    endpoint_addr: u8,
) -> Result<(), usize> {
    // TODO: If no interrupt endpoint use control transfer, otherwise use interrupt endpoing
    // This is what hidapi does
    if USE_CONTROL_TRANSFER {
        let mut request = UsbDeviceRequest {
            request_type: UsbRequestType::HidSetRequest as u8,
            request: HidRequest::SetReportRequest as u8,
            value: ((_report_type as u16) << 8) + _report_id as u16, //u16::from_le_bytes([report_id, report_type as u8]), // TODO: Might be backwards
            index: _interface as u16,
            length: report.len() as u16,
        };
        usb_io.control_transfer(&mut request, UsbDataDirection::DataOut, TIMEOUT, report)
    } else {
        info!("Writing to endpoint: {endpoint_addr:02X}");
        usb_io.sync_interrupt_transfer(endpoint_addr, TIMEOUT, report)
    }
}

fn get_report_request(
    usb_io: &mut UsbIo,
    _interface: u8,
    _report_id: u8,
    _report_type: ReportType,
    report: &mut [u8],
    endpoint_addr: u8,
) -> Result<(), usize> {
    // TODO: If no interrupt endpoint use control transfer, otherwise use interrupt endpoing
    // This is what hidapi does
    if USE_CONTROL_TRANSFER {
        let mut request = UsbDeviceRequest {
            request_type: UsbRequestType::HidGetRequest as u8,
            request: HidRequest::SetReportRequest as u8,
            value: u16::from_le_bytes([_report_id, _report_type as u8]), // TODO: Might be backwards
            index: _interface as u16,
            length: report.len() as u16,
        };
        usb_io.control_transfer(&mut request, UsbDataDirection::DataIn, TIMEOUT, report)
    } else {
        let mut res = Ok(());
        for i in 0..10 {
            res = usb_io.sync_interrupt_transfer(endpoint_addr, TIMEOUT, report);
            info!("get_report_request try: {}: {:?}", i, res);
            if res.is_ok() {
                break;
            }
        }
        res
    }
}

/// Find Interrupt OUT endpoint
pub fn find_interrupt_endpoint(
    usb_handle: &mut ScopedProtocol<UsbIo>,
    endpoints: u8,
) -> Option<(UsbEndpointDescriptor, UsbEndpointDescriptor)> {
    let mut input = None;
    let mut output = None;
    for i in 0..endpoints {
        let endpoint = usb_handle
            .get_endpoint_descriptor(i)
            .expect("Failed to get endpoint descriptor");
        debug!("  Endpoint {i}");

        debug!(
            "    Address           0x{:02X}, Direction IN: {:?}",
            endpoint.endpoint_address,
            endpoint.is_in()
        );

        debug!("  Max packet size: {}", { endpoint.max_packet_size });

        debug!(
            "    Attributes:       0x{:02X}, Interrupt: {:?}",
            endpoint.attributes,
            endpoint.is_interrupt()
        );

        if endpoint.is_interrupt() {
            if endpoint.is_in() {
                input = Some(endpoint);
            } else {
                output = Some(endpoint);
            }
        }
    }

    Some((input?, output?))
}

/// Returns the first usage page
pub fn parse_hid_descriptor(report_desc: &[u8]) -> Option<HidData> {
    let mut usage_page = None;
    let mut report_desc = report_desc;
    let mut hid_element;
    while !report_desc.is_empty() {
        (report_desc, hid_element) = find_next_hid_element(report_desc);
        let hid_element = hid_element.unwrap();

        if VERBOSE {
            info!(
                "    Tag: {:X?} - Data: {:X?}",
                hid_element.tag, hid_element.data
            );
        }
        match hid_element.tag {
            HidTypeTag::Main(_) => {}
            HidTypeTag::Global(GlobalTag::UsagePage) => {
                if usage_page.is_none() {
                    usage_page = hid_element.data
                }
            }
            HidTypeTag::Global(_) => {}
            HidTypeTag::Local(_) => {}
            HidTypeTag::Reserved => {}
        };
    }
    usage_page
}

fn find_next_hid_element(report_desc: &[u8]) -> (&[u8], Option<HidElement>) {
    // Parse first byte
    let size = report_desc[0] & 0b11;
    let hid_type = (report_desc[0] & 0b1100) >> 2;
    let tag = (report_desc[0] & 0b11110000) >> 4;
    let mut report_desc = &report_desc[1..];

    assert!(tag != HID_ELEMENT_TAG_LONG, "Long tag not yet supported");

    let mut hid_elem = HidElement {
        _format: ReportFormat::Short as u16,
        _size: size,
        tag: match hid_type {
            0 => HidTypeTag::Main(match tag {
                // TODO: automatically decode
                8 => MainTag::Input,
                9 => MainTag::Output,
                10 => MainTag::Feature,
                11 => MainTag::Collection,
                12 => MainTag::EndCollection,
                _ => panic!("Invalid"),
            }),
            1 => HidTypeTag::Global(match tag {
                // TODO: automatically decode
                0 => GlobalTag::UsagePage,
                1 => GlobalTag::LogicalMinimum,
                2 => GlobalTag::LogicalMaximum,
                3 => GlobalTag::PhysicalMinimum,
                4 => GlobalTag::PhysicalMaximum,
                5 => GlobalTag::UnitExponent,
                6 => GlobalTag::Unit,
                7 => GlobalTag::ReportSize,
                8 => GlobalTag::ReportId,
                9 => GlobalTag::ReportCount,
                10 => GlobalTag::Push,
                11 => GlobalTag::Pop,
                _ => panic!("Invalid"),
            }),
            2 => HidTypeTag::Local(match tag {
                // TODO: automatically decode
                0 => LocalTag::Usage,
                1 => LocalTag::UsageMinimum,
                2 => LocalTag::UsageMaximum,
                3 => LocalTag::DesignatorIndex,
                4 => LocalTag::DesignatorMinimum,
                5 => LocalTag::DesignatorMaximum,
                7 => LocalTag::StringIndex,
                8 => LocalTag::StringMinimum,
                9 => LocalTag::StringMaximum,
                10 => LocalTag::Delimiter,
                _ => panic!("Invalid"),
            }),
            _ => panic!("Invalid"),
        },
        data: None,
    };

    match size {
        0 => {}
        1 => {
            hid_elem.data = Some(HidData::Uint8(report_desc[0]));
            report_desc = &report_desc[1..];
        }
        2 => {
            hid_elem.data = Some(HidData::Uint16(u16::from_le_bytes(
                (&report_desc[0..2]).try_into().unwrap(),
            )));
            report_desc = &report_desc[2..];
        }
        3 => {
            hid_elem.data = Some(HidData::Uint32(u32::from_le_bytes(
                (&report_desc[0..4]).try_into().unwrap(),
            )));
            report_desc = &report_desc[4..];
        }
        _ => panic!("Unhandled size"),
    };
    (report_desc, Some(hid_elem))
}
