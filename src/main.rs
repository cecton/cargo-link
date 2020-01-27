use cargo_metadata::{Package, PackageId};
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(parse(from_os_str))]
    source: PathBuf,
    #[structopt(parse(from_os_str))]
    destination: PathBuf,
}

fn strip_components_from_url(s: &str) -> Result<String, url::ParseError> {
    let mut url = url::Url::parse(s)?;
    url.set_query(None);
    url.set_fragment(None);

    // NOTE: set_scheme doesn't work here for some reason...
    //       this is probably related https://github.com/servo/rust-url/issues/577
    Ok(url.as_str().trim_start_matches("git+").into())
}

fn unwrap_cargometadata_error(err: cargo_metadata::Error) -> cargo_metadata::Error {
    use cargo_metadata::Error::*;

    match err {
        CargoMetadata { stderr } => {
            eprintln!("{}", stderr);
            std::process::exit(1);
        }
        err => err,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let mut source_cmd = cargo_metadata::MetadataCommand::new();
    source_cmd.manifest_path(opt.source.join("Cargo.toml"));
    let source = source_cmd.exec()?;

    let mut destination_cmd = cargo_metadata::MetadataCommand::new();
    destination_cmd.manifest_path(opt.destination.join("Cargo.toml"));
    let destination = destination_cmd.exec().map_err(unwrap_cargometadata_error)?;

    let source_packages: HashMap<&PackageId, &Package> =
        source.packages.iter().map(|x| (&x.id, x)).collect();
    let destination_packages: HashMap<&PackageId, &Package> =
        destination.packages.iter().map(|x| (&x.id, x)).collect();

    let destination_root_dependencies: HashSet<&str> = destination_packages
        [&destination.resolve.as_ref().unwrap().root.as_ref().unwrap()]
        .dependencies
        .iter()
        .map(|x| x.name.as_str())
        .collect();
    let destination_dependencies: HashSet<_> = destination
        .workspace_members
        .iter()
        .map(|x| {
            destination_packages
                .get(x)
                .unwrap()
                .dependencies
                .iter()
                .map(|x| x.name.as_str())
        })
        .flatten()
        .chain(destination_root_dependencies.into_iter())
        .collect();

    let all_members: Vec<_> = source
        .workspace_members
        .iter()
        .map(|x| source_packages.get(x).unwrap().name.as_str())
        .collect();

    let members_to_override: HashMap<String, Vec<&str>> = destination_packages
        .values()
        .filter(|x| all_members.contains(&x.name.as_str()))
        .map(|x| {
            (
                x.source.as_ref().map(|x| x.to_string()).unwrap(),
                x.name.as_str(),
            )
        })
        .filter(|(source, _)| source.starts_with("git+"))
        .into_group_map();

    for (url, members) in members_to_override {
        let url = strip_components_from_url(url.as_str())?;
        println!("[patch.{:?}]", url);

        for name in members {
            if !destination_dependencies.contains(name) {
                continue;
            }

            let package = source
                .packages
                .iter()
                .find(|x| x.name == name)
                .expect("could not find member in packages");

            println!(
                "{} = {{ path = {:?} }}",
                package.name,
                package.manifest_path.parent().unwrap()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_strip_components_from_url() {
        assert_eq!(
            strip_components_from_url("https://example.org/test?foo=bar&baz=42#location"),
            Ok("https://example.org/test".into())
        );
        assert_eq!(
            strip_components_from_url("git+https://example.org/test?foo=bar&baz=42#location"),
            Ok("https://example.org/test".into())
        );
    }
}
