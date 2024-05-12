/// Calculate the flow required to produce power using the hydropower equation
pub fn inverse_hydropower_calculation(
    power: f64,
    head: f64,
    efficiency: f64,
    flow_unit_conversion: f64,
    energy_unit_conversion: f64,
    density: f64,
) -> f64 {
    power / (energy_unit_conversion * density * 9.81 * head * efficiency * flow_unit_conversion)
}

/// Calculate the produced power from the flow using the hydropower equation
pub fn hydropower_calculation(
    flow: f64,
    head: f64,
    efficiency: f64,
    flow_unit_conversion: f64,
    energy_unit_conversion: f64,
    density: f64,
) -> f64 {
    flow * (energy_unit_conversion * density * 9.81 * head * efficiency * flow_unit_conversion)
}
