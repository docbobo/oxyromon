use super::super::database::*;
use super::*;
use std::path::PathBuf;
use tempfile::{NamedTempFile, TempDir};

#[tokio::test]
async fn test() {
    // given
    let _guard = MUTEX.lock().await;

    let test_directory = Path::new("tests");
    let progress_bar = ProgressBar::hidden();

    let db_file = NamedTempFile::new().unwrap();
    let pool = establish_connection(db_file.path().to_str().unwrap()).await;
    let mut connection = pool.acquire().await.unwrap();

    let rom_directory = TempDir::new_in(&test_directory).unwrap();
    set_rom_directory(PathBuf::from(rom_directory.path()));
    let tmp_directory = TempDir::new_in(&test_directory).unwrap();
    set_tmp_directory(PathBuf::from(tmp_directory.path()));

    let dat_path = test_directory.join("Test System (20210402) (Headered).dat");
    let (datfile_xml, detector_xml) = parse_dat(&progress_bar, &dat_path, true).await.unwrap();

    // when
    import_dat(
        &mut connection,
        &progress_bar,
        &datfile_xml,
        &detector_xml,
        None,
        false,
        false,
    )
    .await
    .unwrap();

    // then
    let systems = find_systems(&mut connection).await;
    assert_eq!(systems.len(), 1);

    let system = systems.first().unwrap();
    assert_eq!(system.name, "Test System (Headered)");

    assert!(find_header_by_system_id(&mut connection, system.id)
        .await
        .is_none());

    assert_eq!(find_games(&mut connection).await.len(), 1);
    assert_eq!(find_roms(&mut connection).await.len(), 1);
}
