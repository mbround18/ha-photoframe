use frame_core::NetworkPhase;

pub trait ProvisioningManager {
    fn ensure_network(&mut self) -> anyhow::Result<NetworkPhase>;
}

#[derive(Default)]
pub struct HostProvisioningManager;

impl ProvisioningManager for HostProvisioningManager {
    fn ensure_network(&mut self) -> anyhow::Result<NetworkPhase> {
        Ok(NetworkPhase::Connected)
    }
}

#[cfg(target_os = "espidf")]
#[derive(Default)]
pub struct EspProvisioningManager;

#[cfg(target_os = "espidf")]
impl ProvisioningManager for EspProvisioningManager {
    fn ensure_network(&mut self) -> anyhow::Result<NetworkPhase> {
        Ok(NetworkPhase::Provisioning)
    }
}

pub fn create_provisioning_manager() -> Box<dyn ProvisioningManager> {
    #[cfg(target_os = "espidf")]
    {
        return Box::new(EspProvisioningManager);
    }

    #[cfg(not(target_os = "espidf"))]
    {
        Box::new(HostProvisioningManager)
    }
}
