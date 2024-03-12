/// Calculate the flow required to produce power using the hydropower equation
pub fn inverse_hydropower_calculation(
    power: f64,
    water_elevation: f64,
    turbine_elevation: f64,
    efficiency: f64,
    flow_unit_conversion: f64,
    energy_unit_conversion: f64,
    density: f64,
) -> f64 {
    let mut head = water_elevation - turbine_elevation;
    if head < 0.0 {
        head = 0.0;
    }
    power / (energy_unit_conversion * density * 9.81 * head * efficiency * flow_unit_conversion)
}
