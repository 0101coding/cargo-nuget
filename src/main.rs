use structopt::StructOpt;
use cargo_toml::Manifest;
use cargo_toml::Value;
use async_std::task;
use std::io::Cursor;
use thiserror::Error;
use std::path::{Path, PathBuf };

fn main() {
    let opt = Opt::from_args();
    println!("{:?}",  opt.subcommand);

    match opt.subcommand {
        Subcommand::Install(_i) => {
            do_install().unwrap();
        }
    }
}


 // A uttility for interactin with nuget packages
#[derive(StructOpt, Debug)]
#[structopt(name = "nuget")]
struct Opt {
    #[structopt(subcommand)]
    pub subcommand: Subcommand,
}

#[derive(Debug, StructOpt)]
enum Subcommand {
    Install(Install), 
}
#[derive(Debug, StructOpt)]
pub struct Install { 
}

fn do_install() -> Result<(), Error> {
     let bytes = std::fs::read("Cargo.toml").map_err(|_| Error::NoCargoToml)?;
     let manifest = Manifest::from_slice(&bytes).map_err(|_| Error::MalformedManifest)?;
     let deps =  get_deps(manifest);
     println!("{:#?}", deps);
     let downloaded = download_dependencies(deps);
     //println!("{:?}", downloaded);
     for (dep, zipped_bytes) in downloaded.unwrap() {
         let mut reader: Cursor<Vec<u8>> = Cursor::new(zipped_bytes);
         let mut zip = zip::ZipArchive::new(reader).map_err(|e| Error::Other(Box::new(e)))?;
         let mut winmds = Vec::new();
         for i in 0..zip.len() {
            let mut file = zip.by_index(i).unwrap();
            let path = file.sanitized_name();
            match path.extension() {
                Some(e) if e == "winmd" => {
                    let name = path.file_name().unwrap().to_owned();
                    let mut contents = Vec::with_capacity(file.size() as usize);
                    use std::io::Read;
                    file.read(&mut contents).unwrap();
                    winmds.push((name, contents ));
                }
                _ => {}
            }
           // Todos Dll
         }
         // Create the dependenncy directory
         let dep_directory = PathBuf::new().join("target").join("nuget").join(dep.name);
         std::fs::create_dir_all(&dep_directory).unwrap();
         for (name, contents) in winmds {
             std::fs::write(dep_directory.join(name), contents).unwrap();
         }
     }
     Ok(())

}

#[derive(Debug, Error)]
enum Error {
    #[error("No cargo.toml could be found")]
    NoCargoToml,
    #[error("There was an error downloading the Nuget Package {0}")]
    DownloadError(Box<dyn std::error::Error>),
    #[error("The Cargo.toml file was malformed")]
    MalformedManifest,
    #[error("There was some other error {0}")]
    Other(Box<dyn std::error::Error>),
}

fn get_deps(manifest: Manifest) -> Vec<Dependency> {
     let metadata = manifest.package.unwrap().metadata.unwrap();
     match metadata {
        Value::Table(mut t) => {
            let deps = match t.remove("nuget_dependencies") {
                Some(Value::Table(deps)) => deps,
                _ => panic!("Not there"),
            }; 
            deps.into_iter()
                .map(|(key, value)| {
                let version = match value {
                    Value::String(version) => version,
                    _ => panic!("Version is not a string")
                };
                Dependency {
                    name: key,
                    version
                }
            }).collect()
        },
        _ => panic!("Ain't no table")
     }
}

#[derive(Debug)]
struct Dependency {
    name: String,
    version: String,
}

impl Dependency {
    fn url(&self) -> String {
        format!("https://www.nuget.org/api/v2/package/{}/{}", self.name, self.version)
    }
}

//type Error = Box<dyn std::error::Error + std::marker::Send + std::marker::Sync>;

fn download_dependencies(deps: Vec<Dependency>) -> Result<Vec<(Dependency, Vec<u8>)>, Error> {
    task::block_on( async {
             let bytes_lists = deps.into_iter().map(|dep| async move{
                    let mut res = surf::get(dep.url()).await.map_err(|e| Error::DownloadError(e))?;
                    println!("Error value: {:?} ", res.status());
                    match res.status().into() {
                        200 => {},
                        302 => {
                            let headers = res.headers();
                            let redirect_url = headers.get("Location").unwrap();
                            res = surf::get(redirect_url).await.unwrap();
                            assert!(res.status() == 200);
                        },
                        _ => return Err(Error::DownloadError(
                                    anyhow::anyhow!("Not 200 response: {}", res.status()).into()
                                ))

                    }
                    let bytes  = res.body_bytes().await.map_err(|e| Error::DownloadError(e.into()))?;
                    Ok((dep, bytes))
            });
                  let result  = futures::future::try_join_all(bytes_lists).await;
                  result
                 
            })

}

