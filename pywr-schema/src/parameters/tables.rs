use crate::digest::Checksum;
#[cfg(feature = "core")]
use crate::error::SchemaError;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta};
#[cfg(all(feature = "core", feature = "hdf5"))]
use crate::timeseries::subset_array2;
use crate::v1::{IntoV2, try_convert_parameter_attr};
use crate::{ComponentConversionError, TryFromV1};
#[cfg(all(feature = "core", feature = "hdf5"))]
use ndarray::s;
#[cfg(all(feature = "core", feature = "hdf5"))]
use pywr_core::parameters::ParameterName;
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::parameters::TablesArrayParameter as TablesArrayParameterV1;
use schemars::JsonSchema;
use std::path::PathBuf;

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct TablesArrayParameter {
    pub meta: ParameterMeta,
    pub node: String,
    #[serde(rename = "where")]
    pub wh: String,
    pub scenario: Option<String>,
    pub checksum: Option<Checksum>,
    pub url: PathBuf,
    pub timestep_offset: Option<i32>,
}

#[cfg(all(feature = "core", feature = "hdf5"))]
impl TablesArrayParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<pywr_core::parameters::ParameterIndex<f64>, SchemaError> {
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

        if let Some(checksum) = &self.checksum {
            checksum.check(&pth)?;
        }

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
                array = subset_array2(&array, subset)?;
            }

            let p = pywr_core::parameters::Array2Parameter::new(
                ParameterName::new(&self.meta.name, parent),
                array,
                scenario_group_index,
                self.timestep_offset,
            );
            Ok(network.add_simple_parameter(Box::new(p))?)
        } else {
            let array = array.slice_move(s![.., 0]);
            let p = pywr_core::parameters::Array1Parameter::new(
                ParameterName::new(&self.meta.name, parent),
                array,
                self.timestep_offset,
            );
            Ok(network.add_simple_parameter(Box::new(p))?)
        }
    }
}

#[cfg(all(feature = "core", not(feature = "hdf5")))]
impl TablesArrayParameter {
    pub fn add_to_model(
        &self,
        _network: &mut pywr_core::network::Network,
        _args: &LoadArgs,
        _parent: Option<&str>,
    ) -> Result<pywr_core::parameters::ParameterIndex<f64>, SchemaError> {
        Err(SchemaError::FeatureNotEnabled("hdf5".to_string()))
    }
}

impl TryFromV1<TablesArrayParameterV1> for TablesArrayParameter {
    type Error = ComponentConversionError;
    fn try_from_v1(
        v1: TablesArrayParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let checksum = match v1.checksum {
            Some(checksum) => Some(try_convert_parameter_attr(
                &meta.name,
                "checksum",
                checksum,
                parent_node,
                conversion_data,
            )?),
            None => None,
        };

        Ok(Self {
            meta,
            node: v1.node,
            wh: v1.wh,
            scenario: v1.scenario,
            checksum,
            url: v1.url,
            timestep_offset: None,
        })
    }
}
