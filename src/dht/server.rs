/// ip protocol stacks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetType {
    Ipv4,
    Ipv6,
    Dual,
}
