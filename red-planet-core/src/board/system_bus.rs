use std::rc::Weak;

use super::PowerDown;
use crate::address_map::TwoWayAddressMap;
use crate::bus::Bus;
use crate::core::clint::Clint;
use crate::interrupt::{DynIrqCallback, IrqCallback};
use crate::resources::plic::Plic;
use crate::resources::ram::Ram;
use crate::resources::rom::Rom;
use crate::resources::uart::Uart;
use crate::system_bus::AccessType;
use space_time::allocator::Allocator;

/// Enum that uniquely identifies every device attached to a [`SystemBus`] (as a slave).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub(super) enum Resource {
    Mrom,
    Clint,
    Plic,
    Uart0,
    Flash,
    Dram,
    PowerDown,
}

/// Abstraction of a system's main bus connecting all devices to the core.
///
/// This can be thought of as a (TileLink) crossbar providing a single *master* interface for the
/// entire 32-bit physical address space, and delegating requests to the appropriate agent's *slave*
/// interface depending on a configurable address mapping.
///
/// Note that vacant memory regions (i.e. unmapped address ranges) are allowed, but accessing them
/// will do nothing.
///
/// Accesses are always in the form of `(address, size)` pairs. The access request is forwarded to
/// the *slave* interface that `address` maps to, if and only if the entire address range
/// `address..(address+size)` is contained within the memory region that `address` is in. Otherwise,
/// the access is not forwarded and will do nothing.
///
/// See also the [`crate::system_bus::SystemBus`] trait.
#[derive(Debug)]
pub(super) struct SystemBus<A: Allocator> {
    pub memory_map: TwoWayAddressMap<Resource>,
    pub mrom: Rom<A>,
    pub clint: Clint<A>,
    pub plic: Plic<A>,
    pub uart0: Uart<A>,
    pub flash: Rom<A>,
    pub dram: Ram<A>,
    pub power_down: PowerDown<A>,
}

struct PlicIrqCallback<A: Allocator> {
    bus: Weak<SystemBus<A>>,
    index: u8,
}

impl<A: Allocator> IrqCallback<A> for PlicIrqCallback<A> {
    fn raise(&self, allocator: &mut A) {
        if let Some(bus) = self.bus.upgrade() {
            bus.plic.raise(allocator, self.index)
        }
    }

    fn lower(&self, allocator: &mut A) {
        if let Some(bus) = self.bus.upgrade() {
            bus.plic.lower(allocator, self.index)
        }
    }
}

impl<A: Allocator> SystemBus<A> {
    /// Validates the `(address, size)` pair, returning `Some((resource, mapped_address))` if the
    /// access is accepted, and `None` otherwise.
    fn check_access(&self, address: u32, size: usize) -> Option<(Resource, u32)> {
        let (range, Some(&resource)) = self.memory_map.range_value(address) else {
            return None;
        };

        if size
            .checked_sub(1)
            .and_then(|delta| u32::try_from(delta).ok())
            .map(|delta| range.end() - address < delta)
            .unwrap_or(true)
        {
            return None;
        }

        Some((resource, address - range.start()))
    }

    fn bus_of(&self, resource: Resource) -> &dyn Bus<A> {
        match resource {
            Resource::Mrom => &self.mrom,
            Resource::Clint => &self.clint,
            Resource::Plic => &self.plic,
            Resource::Uart0 => &self.uart0,
            Resource::Flash => &self.flash,
            Resource::Dram => &self.dram,
            Resource::PowerDown => &self.power_down,
        }
    }

    /// Panics if `index` is not in 1..=52
    pub fn get_plic_irq_callback(bus: Weak<Self>, index: u8) -> DynIrqCallback<A> {
        if !(1..=52).contains(&index) {
            panic!("Invalid interrupt idx: {index}");
        }

        DynIrqCallback(Box::new(PlicIrqCallback { bus, index }))
    }

    pub(super) fn drop(self, allocator: &mut A) {
        self.mrom.drop(allocator);
        self.clint.drop(allocator);
        self.plic.drop(allocator);
        self.uart0.drop(allocator);
        self.flash.drop(allocator);
        self.dram.drop(allocator);
    }
}

impl<A: Allocator> crate::system_bus::SystemBus<A> for SystemBus<A> {
    fn accepts(&self, address: u32, size: usize, access_type: AccessType) -> bool {
        let Some((resource, _)) = self.check_access(address, size) else {
            return false;
        };

        match resource {
            Resource::Mrom => !matches!(access_type, AccessType::Write),
            Resource::Clint => size == 4 || size == 8,
            Resource::Plic => size == 4,
            Resource::Uart0 => true,
            Resource::Flash => !matches!(access_type, AccessType::Write),
            Resource::Dram => true,
            Resource::PowerDown => matches!(access_type, AccessType::Write),
        }
    }
}

impl<A: Allocator> Bus<A> for SystemBus<A> {
    fn read(&self, buf: &mut [u8], allocator: &mut A, address: u32) {
        // If no region is being accessed, or the access is not valid, nothing happens.
        if let Some((resource, mapped_address)) = self.check_access(address, buf.len()) {
            self.bus_of(resource).read(buf, allocator, mapped_address);
        }
    }

    fn read_debug(&self, buf: &mut [u8], allocator: &A, address: u32) {
        // If no region is being accessed, or the access is not valid, nothing happens.
        if let Some((resource, mapped_address)) = self.check_access(address, buf.len()) {
            self.bus_of(resource)
                .read_debug(buf, allocator, mapped_address)
        }
    }

    fn write(&self, allocator: &mut A, address: u32, buf: &[u8]) {
        // If no region is being accessed, or the access is not valid, nothing happens.
        if let Some((resource, mapped_address)) = self.check_access(address, buf.len()) {
            self.bus_of(resource).write(allocator, mapped_address, buf);
        }
    }
}
