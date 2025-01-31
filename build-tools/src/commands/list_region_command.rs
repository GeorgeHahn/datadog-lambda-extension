use std::io::Result;
use std::io::{Error, ErrorKind};

use aws_sdk_ec2 as ec2;
use structopt::StructOpt;

use super::common::build_config;

#[derive(Debug, StructOpt)]
pub struct ListRegionOptions {
    #[structopt(long)]
    pub assume_role: Option<String>,
    #[structopt(long)]
    pub external_id: Option<String>,
}

pub async fn list_region(args: &ListRegionOptions) -> Result<()> {
    // set a random AWS_REGION as ec2:DescribeRgions in region-agnostic
    let config = build_config(
        "us-east-1",
        args.assume_role.clone(),
        args.external_id.clone(),
    )
    .await;
    let ec2_client = ec2::Client::new(&config);
    let regions = get_list(&ec2_client).await?;
    output_region(&regions)?;
    Ok(())
}

async fn get_list(ec2_client: &ec2::Client) -> Result<Vec<String>> {
    let result = ec2_client
        .describe_regions()
        .send()
        .await
        .expect("could not list regions");

    if let Some(regions) = result.regions() {
        let result: Vec<_> = regions.iter().map(|region| region.region_name()).collect();

        let result = result
            .iter()
            .flatten()
            .map(|region_name| String::from(*region_name))
            .collect::<Vec<_>>();
        Ok(result)
    } else {
        Err(Error::new(ErrorKind::InvalidData, "could not get regions"))
    }
}

fn output_region(regions: &Vec<String>, ) -> Result<()> {
    let mut first = true;
    for region in regions.iter() {
        if !first {
            print!(",");
        }
        first = false;
        print!("{}", region);
    }
    Ok(())
}
