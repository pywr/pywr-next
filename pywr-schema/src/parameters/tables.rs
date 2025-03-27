#[cfg(feature = "core")]
use crate::error::SchemaError;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta};
#[cfg(feature = "core")]
use crate::timeseries::subset_array2;
use crate::v1::{FromV1, IntoV2};
#[cfg(feature = "core")]
use ndarray::s;
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterIndex;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::TablesArrayParameter as TablesArrayParameterV1;
use schemars::JsonSchema;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct TablesArrayParameter {
    pub meta: ParameterMeta,
    pub node: String,
    #[serde(rename = "where")]
    pub wh: String,
    pub scenario: Option<String>,
    pub checksum: Option<HashMap<String, String>>,
    pub url: PathBuf,
    pub timestep_offset: Option<i32>,
}

#[cfg(feature = "core")]
impl TablesArrayParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        // 1. Load the file from the HDF5 file (NB this is not Pandas format).

        // Handle the case of an optional data path with a relative url.
        let pth = if let Some(dp) = args.data_path {
            if self.url.is_relative() {
                dp.join(&self.url)
            } else {
                self.url.clone()
            }
        } else {
            self.url.clone()
        };

        let file = hdf5_metno::File::open(pth).map_err(|e| SchemaError::HDF5Error(e.to_string()))?; // open for reading

        let grp = file
            .group(&self.wh)
            .map_err(|e| SchemaError::HDF5Error(e.to_string()))?; // find the group
        let ds = grp
            .dataset(&self.node)
            .map_err(|e| SchemaError::HDF5Error(e.to_string()))?; // find the dataset

        let mut array = ds.read_2d::<f64>().map_err(|e| SchemaError::HDF5Error(e.to_string()))?;
        // 2. TODO Validate the shape of the data array. I.e. check number of columns matches scenario
        //    and number of rows matches time-steps.

        // 3. Create an ArrayParameter using the loaded array.
        if let Some(scenario) = &self.scenario {
            let scenario_group_index = args.domain.scenarios().group_index(scenario)?;

            // If there is a scenario subset then we can reduce the data to align with the scenarios
            // that are actually used in the model.
            if let Some(subset) = args.domain.scenarios().group_scenario_subset(scenario)? {
                array = subset_array2(&array, subset);
            }

            let p = pywr_core::parameters::Array2Parameter::new(
                self.meta.name.as_str().into(),
                array,
                scenario_group_index,
                self.timestep_offset,
            );
            Ok(network.add_simple_parameter(Box::new(p))?)
        } else {
            let array = array.slice_move(s![.., 0]);
            let p = pywr_core::parameters::Array1Parameter::new(
                self.meta.name.as_str().into(),
                array,
                self.timestep_offset,
            );
            Ok(network.add_simple_parameter(Box::new(p))?)
        }
    }
}

impl FromV1<TablesArrayParameterV1> for TablesArrayParameter {
    fn from_v1(v1: TablesArrayParameterV1, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> Self {
        Self {
            meta: v1.meta.into_v2(parent_node, conversion_data),
            node: v1.node,
            wh: v1.wh,
            scenario: v1.scenario,
            checksum: v1.checksum,
            url: v1.url,
            timestep_offset: None,
        }
    }
}
