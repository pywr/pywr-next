use crate::schema::parameters::{DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter};
use crate::{ParameterIndex, PywrError};
use ndarray::{arr2, arr3, s, ArrayBase, DataMut, Dimension, NewAxis, Slice};
use pywr_schema::parameters::TablesArrayParameter as TablesArrayParameterV1;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct TablesArrayParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub node: String,
    #[serde(rename = "where")]
    pub wh: String,
    pub scenario: Option<String>,
    pub checksum: Option<HashMap<String, String>>,
    pub url: PathBuf,
}

impl TablesArrayParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(
        &self,
        model: &mut crate::model::Model,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        // 1. Load the file from the HDF5 file (NB this is not Pandas format).

        // Handle the case of an optional data path with a relative url.
        let pth = if let Some(dp) = data_path {
            if self.url.is_relative() {
                dp.join(&self.url)
            } else {
                self.url.clone()
            }
        } else {
            self.url.clone()
        };

        let file = hdf5::File::open(&pth).map_err(|e| PywrError::HDF5Error(e.to_string()))?; // open for reading

        let grp = file.group(&self.wh).map_err(|e| PywrError::HDF5Error(e.to_string()))?; // find the group
        let ds = grp
            .dataset(&self.node)
            .map_err(|e| PywrError::HDF5Error(e.to_string()))?; // find the dataset

        let array = ds.read_2d::<f64>().map_err(|e| PywrError::HDF5Error(e.to_string()))?;
        // 2. TODO Validate the shape of the data array. I.e. check number of columns matches scenario
        //    and number of rows matches time-steps.
        let array = array.slice_move(s![.., 0]);

        // 3. Create an ArrayParameter using the loaded array.
        let p = crate::parameters::Array1Parameter::new(&self.meta.name, array);
        model.add_parameter(Box::new(p))
    }
}

impl TryFromV1Parameter<TablesArrayParameterV1> for TablesArrayParameter {
    type Error = PywrError;

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
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct DataframeParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    #[serde(rename = "where")]
    pub scenario: Option<String>,
    pub url: String,
}

impl DataframeParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<ParameterIndex, PywrError> {
        // 1. Call Python & Pandas to read the data

        // 2. Validate the shape of the data array. I.e. check number of columns matches scenario
        //    and number of rows matches time-steps.

        // 3. Create an ArrayParameter using the loaded array.

        todo!("See above comments")
    }
}
