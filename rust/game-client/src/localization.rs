//! Russian-first, dependency-free localization for the playable Rust client.
//! Original GD/OD/ODB asset records are never modified.

use std::borrow::Cow;
use game_core::sim::{GameError,Task};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Language { Russian, English }
impl Language {
    pub fn toggle(self)->Self{
        match self {Self::Russian=>Self::English,Self::English=>Self::Russian}
    }
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Key {
    GameTitle,Campaign,BuildMenu,Selected,ChooseObject,CancelBuild,
    House,TownHall,Warehouse,Map,Race,Type,Health,Task,Progress,
    Paused,Running,Gold,Wood,Stone,Food,Tick,PriceLegend,Help1,Help2,
}
pub fn tr(lang:Language,key:Key)->&'static str{
    use Key::*;
    match (lang,key) {
        (Language::Russian,GameTitle)=>"THE LOST WORLD  |  RUST",
        (Language::Russian,Campaign)=>"КАМПАНИЯ",
        (Language::Russian,BuildMenu)=>"СТРОИТЕЛЬСТВО",
        (Language::Russian,Selected)=>"ВЫБРАНО",
        (Language::Russian,ChooseObject)=>"Выберите объект на карте",
        (Language::Russian,CancelBuild)=>"0 — ОТМЕНА",
        (Language::Russian,House)=>"1 — ЖИЛОЙ ДОМ",
        (Language::Russian,TownHall)=>"2 — РАТУША",
        (Language::Russian,Warehouse)=>"3 — СКЛАД",
        (Language::Russian,Map)=>"Карта",
        (Language::Russian,Race)=>"Народ",
        (Language::Russian,Type)=>"Тип",
        (Language::Russian,Health)=>"Здоровье",
        (Language::Russian,Task)=>"Задание",
        (Language::Russian,Progress)=>"Построено",
        (Language::Russian,Paused)=>"ПАУЗА",
        (Language::Russian,Running)=>"ИГРА ИДЁТ",
        (Language::Russian,Gold)=>"ЗОЛОТО",
        (Language::Russian,Wood)=>"ДЕРЕВО",
        (Language::Russian,Stone)=>"КАМЕНЬ",
        (Language::Russian,Food)=>"ЕДА",
        (Language::Russian,Tick)=>"ТИК",
        (Language::Russian,PriceLegend)=>"Д / К / Е",
        (Language::Russian,Help1)=>"ЛКМ: выбор/рамка   SHIFT: добавить   CTRL: переключить   ПКМ: приказ",
        (Language::Russian,Help2)=>"WASD: карта   КОЛЕСО: масштаб   P: пауза   F4: мини-карта   F5/F9: сохранение   F10: язык",
        (Language::English,GameTitle)=>"THE LOST WORLD  |  RUST EDITION",
        (Language::English,Campaign)=>"CAMPAIGN",
        (Language::English,BuildMenu)=>"BUILD MENU",
        (Language::English,Selected)=>"SELECTED",
        (Language::English,ChooseObject)=>"Click an object on the map",
        (Language::English,CancelBuild)=>"0 - CANCEL BUILD",
        (Language::English,House)=>"1 - HOUSE",
        (Language::English,TownHall)=>"2 - TOWN HALL",
        (Language::English,Warehouse)=>"3 - WAREHOUSE",
        (Language::English,Map)=>"Map",
        (Language::English,Race)=>"Race",
        (Language::English,Type)=>"Type",
        (Language::English,Health)=>"HP",
        (Language::English,Task)=>"Task",
        (Language::English,Progress)=>"Construction",
        (Language::English,Paused)=>"PAUSED",
        (Language::English,Running)=>"RUNNING",
        (Language::English,Gold)=>"GOLD",
        (Language::English,Wood)=>"WOOD",
        (Language::English,Stone)=>"STONE",
        (Language::English,Food)=>"FOOD",
        (Language::English,Tick)=>"TICK",
        (Language::English,PriceLegend)=>"W / S / F",
        (Language::English,Help1)=>"LMB: select/drag   SHIFT: add   CTRL: toggle   RMB: order",
        (Language::English,Help2)=>"WASD: pan   WHEEL: zoom   P: pause   F4: map   F5/F9: save/load   F10: language",
    }
}
pub fn task_name(lang:Language,task:&Task)->&'static str{
    match lang{
        Language::Russian=>match task{
            Task::Idle=>"Свободен",
            Task::Walking{..}=>"Идёт",
            Task::Gathering{..}=>"Добывает ресурс",
            Task::Delivering{..}=>"Несёт на склад",
            Task::Constructing{..}=>"Строит",
            Task::Holding{..}=>"Несёт груз (нет склада)",
        },
        Language::English=>match task{
            Task::Idle=>"Idle",
            Task::Walking{..}=>"Moving",
            Task::Gathering{..}=>"Gathering",
            Task::Delivering{..}=>"Delivering",
            Task::Constructing{..}=>"Building",
            Task::Holding{..}=>"Carrying / no storage",
        }
    }
}
pub fn error_name(lang:Language,error:&GameError)->Cow<'static,str>{
    if lang==Language::English{return Cow::Owned(error.to_string())}
    Cow::Borrowed(match error{
        GameError::Invalid(_)=>"Недопустимая команда",
        GameError::NoPath=>"Нет доступного пути",
        GameError::MissingObject=>"Объект не найден",
        GameError::NotOwned=>"Это не ваш персонаж",
        GameError::NotEnoughGoods=>"Недостаточно ресурсов",
        GameError::Occupied=>"Место занято",
        GameError::NotLand=>"Нельзя строить на этой местности",
        GameError::Busy=>"Рабочий занят",
        GameError::TownHallExists=>"Ратуша уже построена",
        GameError::TooManyEntities=>"Превышен лимит объектов",
    })
}
pub fn object_name<'a>(
    lang:Language,name:&'a str,kind:i32,ty:i32,class:i32,
)->Cow<'a,str>{
    if lang==Language::English{return Cow::Borrowed(name)}
    let translated=match kind{
        // Same object type IDs mean the same role across all three races
        // (see docs/formats/odb.md, "Groups and types").
        0=>match ty{
            9=>Some("Носильщик"),10=>Some("Основатель поселения"),
            11=>Some("Строитель"),12=>Some("Преступник"),
            13=>Some("Дипломат"),14=>Some("Земледелец"),
            17=>Some("Охотник"),19=>Some("Лесоруб"),
            20=>Some("Разносчик газет"),21=>Some("Полицейский"),
            27=>Some("Каменотёс"),29=>Some("Лучник"),
            _=>None,
        },
        1=>match ty{
            37=>Some("Жилой дом"),38=>Some("Большой жилой дом"),
            39=>Some("Казармы"),40=>Some("Ратуша"),
            41=>Some("Склад"),42=>Some("Посольство"),
            43=>Some("Военная мастерская"),44=>Some("Пищевое производство"),
            46=>Some("Лаборатория"),48=>Some("Рынок"),
            50=>Some("Шахта"),52=>Some("Малый форпост"),
            53=>Some("Большой форпост"),54=>Some("Полицейский участок"),
            55=>Some("Школа"),56=>Some("Магическая мастерская"),
            61=>Some("Оружейная мастерская"),62=>Some("Порт"),
            63=>Some("Верфь"),_=>None,
        },
        2=>match class{
            7=>Some("Дикое животное"),8=>Some("Дерево"),
            9=>Some("Ягодный куст"),13=>Some("Каменное месторождение"),
            10=>Some("Плантация"),11=>Some("Грибы"),
            12=>Some("Личинки"),_=>None,
        },
        _=>None,
    };
    match translated{
        Some(text)=>Cow::Borrowed(text),
        None=>Cow::Borrowed(name), // keep unknown original names until translated
    }
}
pub fn mission_title<'a>(lang:Language,title:&'a str)->Cow<'a,str>{
    if lang==Language::Russian && title=="Die Trockenperiode"{
        Cow::Borrowed("Засуха")
    }else{Cow::Borrowed(title)}
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn russian_is_complete_for_all_current_hud_labels(){
        use Key::*;
        let all=[GameTitle,Campaign,BuildMenu,Selected,ChooseObject,
            CancelBuild,House,TownHall,Warehouse,Map,Race,Type,Health,Task,
            Progress,Paused,Running,Gold,Wood,Stone,Food,Tick,PriceLegend,
            Help1,Help2];
        for key in all{
            assert!(!tr(Language::Russian,key).is_empty());
            assert!(!tr(Language::English,key).is_empty());
        }
    }
    #[test]
    fn errors_and_object_names_are_translated(){
        assert_eq!(error_name(Language::Russian,&GameError::Occupied),"Место занято");
        assert_eq!(object_name(Language::Russian,"Rathaus",1,40,0),"Ратуша");
        assert_eq!(object_name(Language::Russian,"Baumeister",0,11,0),"Строитель");
        assert_eq!(object_name(Language::Russian,"Example",2,42,8),"Дерево");
        assert_eq!(object_name(Language::Russian,"Holzfäller",0,19,0),"Лесоруб");
        assert_eq!(object_name(Language::Russian,"Botschaft",1,42,0),"Посольство");
        assert_eq!(object_name(Language::Russian,"Unknown",1,999,0),"Unknown");
        assert_eq!(object_name(Language::English,"Rathaus",1,40,0),"Rathaus");
        assert_eq!(Language::Russian.toggle(),Language::English);
    }
}
