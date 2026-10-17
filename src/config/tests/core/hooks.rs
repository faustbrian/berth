use super::*;

#[test]
fn load_config_with_parses_service_hooks() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("berth-config-hooks-{nonce}"));
    std::fs::create_dir_all(&root).expect("create temp config directory");

    let config_path = root.join(".berth.yaml");
    project_file::write_test_config_file(
        &config_path,
        r#"
                project_type = "project"
                container_prefix = "test"

                [[service]]
                name = "app"
                kind = "app"
                driver = "frankenphp"
                image = "dunglas/frankenphp:php8.5"
                host = "127.0.0.1"
                port = 8000
                domain = "app.berth.localhost"

                [[service.hook]]
                name = "seed-dev-user"
                phase = "post_up"
                on_error = "fail"

                [service.hook.run]
                type = "exec"
                argv = ["php", "artisan", "db:seed", "--class=DevUserSeeder"]

                [[service.hook]]
                name = "clean-cache"
                phase = "post_down"
                on_error = "warn"

                [service.hook.run]
                type = "script"
                path = ".berth/hooks/clean-cache.sh"
            "#,
    )
    .expect("write hooks config");

    let loaded = load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
        .expect("load hooks config");
    assert_eq!(loaded.service.len(), 1);
    let hooks = &loaded.service[0].hook;
    assert_eq!(hooks.len(), 2);
    assert_eq!(hooks[0].name, "seed-dev-user");
    assert_eq!(hooks[0].phase, HookPhase::PostUp);
    assert_eq!(hooks[0].on_error, HookOnError::Fail);
    assert_eq!(
        hooks[0].run,
        HookRun::Exec {
            argv: vec![
                "php".to_owned(),
                "artisan".to_owned(),
                "db:seed".to_owned(),
                "--class=DevUserSeeder".to_owned(),
            ],
        }
    );
    assert_eq!(hooks[1].name, "clean-cache");
    assert_eq!(hooks[1].phase, HookPhase::PostDown);
    assert_eq!(hooks[1].on_error, HookOnError::Warn);
    assert_eq!(
        hooks[1].run,
        HookRun::Script {
            path: ".berth/hooks/clean-cache.sh".to_owned(),
        }
    );

    std::fs::remove_dir_all(root).expect("cleanup temp config directory");
}
