using Godot;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace Deuteros.Code.Platform.Screens.ShipBayScenes
{
    public partial class Engine : Control
    {
        public Control SpriteHolder { get; set; }
        public Control EngineHolder { get; set; }
        public TextureButton InstallEngineButton { get; set; }
        public bool Installed { get; set; }
        public Enums.Ship_Types Shiptype { get; set; }
        public bool Ground { get; set; }
        public Enums.ItemTypes EngineType { get; set; }

        public delegate void EngineInstalledDelegate();
        public event EngineInstalledDelegate EngineInstalled;

        public override void _Ready()
        {
            SpriteHolder = GetNode<Control>("SpriteHolder");
            EngineHolder = GetNode<Control>("SpriteHolder/EngineHolder");
            InstallEngineButton = GetNode<TextureButton>("SpriteHolder/Buttons/InstallEngine");

            InstallEngineButton.Pressed += InstallEngine;
        }

        public void UpdateState()
        {
            EngineHolder.Visible = Installed;
        }

        public void InstallEngine()
        {
            if (!Installed && ((Ground && GameCore.SingletonInstance.GetCurrentPlanet().PlanetResources.Stores[EngineType] > 0) || (!Ground && GameCore.SingletonInstance.GetCurrentPlanet().Station.Resources.Stores[EngineType] > 0)))
            {
                Installed = true;
                EngineInstalled.Invoke();
                UpdateState();
            }
        }
    }
}