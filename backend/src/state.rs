/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-10 10:09:22
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2026-04-18 20:15:00
 * @FilePath: /udx710-backend/backend/src/state.rs
 * @Description:
 *
 * Copyright (c) 2025 by 1orz, All Rights Reserved.
 */

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use axum::extract::FromRef;
use zbus::Connection;

use crate::config::ConfigManager;
use crate::db::Database;
use crate::sms_push::SmsPushSender;
use crate::webhook::WebhookSender;

pub struct FrontendRuntime {
    last_seen: RwLock<Option<Instant>>,
}

impl FrontendRuntime {
    pub fn new() -> Self {
        Self {
            last_seen: RwLock::new(None),
        }
    }

    pub fn mark_seen(&self) {
        *self.last_seen.write().unwrap() = Some(Instant::now());
    }

    pub fn is_recent(&self, timeout: Duration) -> bool {
        self.last_seen
            .read()
            .unwrap()
            .is_some_and(|last_seen| last_seen.elapsed() <= timeout)
    }
}

#[derive(Clone)]
pub struct AppState {
    pub dbus_conn: Arc<Connection>,
    pub database: Arc<Database>,
    pub config_manager: Arc<ConfigManager>,
    pub webhook_sender: Arc<WebhookSender>,
    pub sms_push_sender: Arc<SmsPushSender>,
    pub frontend_runtime: Arc<FrontendRuntime>,
}

impl AppState {
    pub fn new(
        dbus_conn: Arc<Connection>,
        database: Arc<Database>,
        config_manager: Arc<ConfigManager>,
        webhook_sender: Arc<WebhookSender>,
        sms_push_sender: Arc<SmsPushSender>,
        frontend_runtime: Arc<FrontendRuntime>,
    ) -> Self {
        Self {
            dbus_conn,
            database,
            config_manager,
            webhook_sender,
            sms_push_sender,
            frontend_runtime,
        }
    }
}

impl FromRef<AppState> for Arc<Connection> {
    fn from_ref(state: &AppState) -> Self {
        state.dbus_conn.clone()
    }
}

impl FromRef<AppState> for Arc<Database> {
    fn from_ref(state: &AppState) -> Self {
        state.database.clone()
    }
}

impl FromRef<AppState> for Arc<ConfigManager> {
    fn from_ref(state: &AppState) -> Self {
        state.config_manager.clone()
    }
}

impl FromRef<AppState> for Arc<WebhookSender> {
    fn from_ref(state: &AppState) -> Self {
        state.webhook_sender.clone()
    }
}

impl FromRef<AppState> for Arc<SmsPushSender> {
    fn from_ref(state: &AppState) -> Self {
        state.sms_push_sender.clone()
    }
}

impl FromRef<AppState> for Arc<FrontendRuntime> {
    fn from_ref(state: &AppState) -> Self {
        state.frontend_runtime.clone()
    }
}

impl FromRef<AppState> for (Arc<Connection>, Arc<Database>) {
    fn from_ref(state: &AppState) -> Self {
        (state.dbus_conn.clone(), state.database.clone())
    }
}
