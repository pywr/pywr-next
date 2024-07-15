use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::parameters::{IntoV2Parameter, ParameterMeta, TryFromV1Parameter};
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
pub struct TablesArrayParameter {
    #[serde(flatten)]
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

        let file = hdf5::File::open(pth).map_err(|e| SchemaError::HDF5Error(e.to_string()))?; // open for reading

        let grp = file
            .group(&self.wh)
            .map_err(|e| SchemaError::HDF5Error(e.to_string()))?; // find the group
        let ds = grp
            .dataset(&self.node)
            .map_err(|e| SchemaError::HDF5Error(e.to_string()))?; // find the dataset

        let array = ds.read_2d::<f64>().map_err(|e| SchemaError::HDF5Error(e.to_string()))?;
        // 2. TODO Validate the shape of the data array. I.e. check number of columns matches scenario
        //    and number of rows matches time-steps.

        // 3. Create an ArrayParameter using the loaded array.
        if let Some(scenario) = &self.scenario {
            let scenario_group_index = args
                .domain
                .scenarios()
                .group_index(scenario)
                .ok_or(SchemaError::ScenarioGroupNotFound(scenario.to_string()))?;

            let p = pywr_core::parameters::Array2Parameter::new(
                &self.meta.name,
                array,
                scenario_group_index,
                self.timestep_offset,
            );
            Ok(network.add_parameter(Box::new(p))?)
        } else {
            let array = array.slice_move(s![.., 0]);
            let p = pywr_core::parameters::Array1Parameter::new(&self.meta.name, array, self.timestep_offset);
            Ok(network.add_parameter(Box::new(p))?)
        }
    }
}

impl TryFromV1Parameter<TablesArrayParameterV1> for TablesArrayParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: TablesArrayParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let p = Self {
            meta: v1.meta.into_v2_parameter(parent_node, unnamed_count),
            node: v1.node,
            wh: v1.wh,
            scenario: v1.scenario,
            checksum: v1.checksum,
            url: v1.url,
            timestep_offset: None,
        };
        Ok(p)
    }
}
